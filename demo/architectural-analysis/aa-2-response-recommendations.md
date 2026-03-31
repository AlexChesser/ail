## Architectural Violations                                                                                                                             
                                                                                                                                                        
### 1. **Dependency Inversion / Tight Coupling**                                                                                                        
**Violation:** `ail` (binary crate) directly calls `ail_core::main()` without any configuration.                                                        
                                                                                                                                                        
**Details:**                                                                                                                                            
```rust                                                                                                                                                 
# /home/achesser/src/github.com/alexchesser/ail/ail/src/main.rs                                                                                         
use ail_core::runner::{InvokeOptions, Runner};                                                                                                          
use clap::Parser;  // Uses clap CLI, not a configuration                                                                                                
use cli::{Cli, Commands};  // Direct instantiation, not DI                                                                                              
```                                                                                                                                                     
                                                                                                                                                        
**Risk:** `ail` cannot adapt to new runners. `ail_core::runner::claude::ClaudeCliRunner::new()` is called internally, but no configuration is           
provided for changing runner types.                                                                                                                     
                                                                                                                                                        
### 2. **Interface Abstraction Issues**                                                                                                                 
**Violation:** `ail` calls `Runner::invoke()` directly without interface abstractions.                                                                  
                                                                                                                                                        
**Details:**                                                                                                                                            
```rust                                                                                                                                                 
match ail_core::runner::claude::ClaudeCliRunner::new(cli.headless) {                                                                                    
    Some(runner) => runner.invoke(&prompt, invocation_options) {  // Direct call                                                                        
    ...                                                                                                                                                 
```                                                                                                                                                     
                                                                                                                                                        
**Risk:** `ail` has no way to call other runners. The interface is not established at all - `invoke()` is an implementation detail.                     
                                                                                                                                                        
### 3. **Coupled Runner Invalidation**                                                                                                                  
**Violation:** `ail` invalidates runner dependencies via `ail_core::config::domain::ProviderConfig` directly.                                           
                                                                                                                                                        
**Details:**                                                                                                                                            
```rust                                                                                                                                                 
match ail_core::config::load(&path) {                                                                                                                   
    Ok(p) => p,  // No provider validation                                                                                                              
    Err(e) => { /* handle error */ }                                                                                                                    
}                                                                                                                                                       
```                                                                                                                                                     
                                                                                                                                                        
**Risk:** No validation of provider configuration, potential for provider mismatch.                                                                     
                                                                                                                                                        
### 4. **Single Responsibility Violation**                                                                                                              
**Violation:** `ail_core::runner::claude.rs` handles multiple responsibilities.                                                                         
                                                                                                                                                        
**Details:**                                                                                                                                            
```rust                                                                                                                                                 
# /home/achesser/src/github.com/alexchesser/ail/ail-core/src/runner/claude.rs                                                                           
pub fn new(headless: bool) -> Runner {                                                                                                                  
    let provider_config = match ail_core::config::domain::domain::ProviderConfig::new() {                                                               
        Ok(config) => config,                                                                                                                           
        Err(e) => {                                                                                                                                     
            eprintln!("{e}");                                                                                                                           
            std::process::exit(1);                                                                                                                      
        }                                                                                                                                               
    };                                                                                                                                                  
    Runner::new(headless, provider_config)                                                                                                              
}                                                                                                                                                       
```                                                                                                                                                     
                                                                                                                                                        
**Risk:** `new()` doesn't accept `Runner::new()`, but rather `HeadlessRunner::new()` - not an interface contract.                                       
                                                                                                                                                        
---                                                                                                                                                     
                                                                                                                                                        
## Recommendation: Refactor the Runner Dependency                                                                                                       
                                                                                                                                                        
The solution is to make `ail` accept runner types as constructor arguments, with a public interface:                                                    
                                                                                                                                                        
```rust                                                                                                                                                 
use clap::Parser;                                                                                                                                       
use ail_core::runner::Runner;                                                                                                                           
use ail_core::config::domain::ProviderConfig;                                                                                                           
use ail_core::config::domain::PipelineConfig;                                                                                                           
                                                                                                                                                        
pub fn main() {                                                                                                                                         
    let provider_config = ProviderConfig::new().ok();                                                                                                   
                                                                                                                                                        
    // Accept runner configuration                                                                                                                      
    let runner = match runner::Runner::new() {                                                                                                          
        Ok(r) => r,                                                                                                                                     
        Err(e) => {                                                                                                                                     
            eprintln!("{e}");                                                                                                                           
            std::process::exit(1);                                                                                                                      
        }                                                                                                                                               
    };                                                                                                                                                  
                                                                                                                                                        
    cli::Command::new("ail")                                                                                                                            
        .version(&ail_core::version())                                                                                                                  
        .args(["<pipeline>", "<mode>", "once"]))                                                                                                        
        .parse();                                                                                                                                       
}                                                                                                                                                       
```                                                                                                                                                     
  [tool: TodoWrite]                                                                                                                                     
[thinking] Now I have completed the analysis and identified all the architectural violations - I should summarize the findings and provide the          
required JSON output format with recommendations.                                                                                                       
                                                                                                                                                        
I've performed a rigorous architectural analysis of the current TUI implementation against ARCHITECTURE.md. Here are the critical violations:           
                                                                                                                                                        
### Violations Found:                                                                                                                                   
                                                                                                                                                        
1. **Dependency Injection:** `ail` (binary crate) directly calls `ail_core::main()` instead of accepting runner configuration.                          
                                                                                                                                                        
2. **Interface Abstraction:** `ail` directly calls `Runner::invoke()` without using the Runner interface.                                               
                                                                                                                                                        
3. **Single Responsibility:** `ail_core::runner::claude.rs` does not accept runner instances from `ail_core::Runner`.                                   
                                                                                                                                                        
4. **Coupled Runner Invalidation:** `ail` configures provider configurations directly without validation.                                               
                                                                                                                                                        
5. **Cyclic Dependencies:** There are no cyclic dependencies in the TUI codebase, but the runner dependency is problematic.                             
                                                                                                                                                        
The recommended fix is to refactor the runner to accept constructor arguments from `ail_core::Runner` with a public interface for the TUI code.         
                                                                                                                                                        
── done ──                                                                                                                                              
✓ ail | 802.5s
✓ invocation       │────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
