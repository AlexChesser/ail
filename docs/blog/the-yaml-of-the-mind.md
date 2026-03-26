# The YAML of the Mind
## *What the science of cognitive psychology and education tells us about modeling executive function for agentic LLMs*

---

## The Eight Conclusions — Read This If Nothing Else

* **Your agent's failure modes have clinical names.** From Harlow's 1848 iron bar case, through Luria's 1966 documentation of the frontal syndrome, to Baddeley naming it Dysexecutive Syndrome in 1986 — your frustrations have a sixty-year diagnosis.
* **What's missing is Executive Function** — the cognitive system responsible for inhibiting wrong responses, managing working memory, and shifting between strategies. You can wait for it to emerge from within the unseen layers of a neural network, or you can build a frontal lobe.
* **The skills ecosystem operates at the wrong layer.** Adding more natural language instructions to a context saturation problem just makes a larger context. `ail` operates at the layer that decides what goes into the context at all. The specification and reference implementation are a work in progress at https://github.com/AlexChesser/ail.
* **LLM agents lack a discrete selection mechanism for task-relevant data.** In Baddeley’s (1986) model of working memory, the "Central Executive" coordinates the active information; storage happens elsewhere. Without this first-class primitive, an agent’s context becomes a soup of cross-contamination: feature requirements bleed into test-writing, and debugging heuristics interfere with architectural constraints. What looks like "model confusion" is actually a predictable system failure—the inability to selectively inhibit irrelevant data during a specific sub-task.
* **A YAML specification is a cognitive artifact** — an externalization of expertise into a medium that is simultaneously executable, version-controllable, and legible to both the human architect and the model.
* **Named, inheritable routines are "scripts" in the 1977 Schank and Abelson sense:** pre-compiled procedural schemas that an intelligent system activates rather than derives. Intelligence requires knowing which script to run.
* **LLMs suffer from a specific deficit in metacognitive monitoring.** First identified by Flavell (1979), this is the capacity to evaluate one’s own cognitive processes in real-time. In clinical neurology, the total absence of this internal error-correction signal is known as anosognosia (Babinski, 1914)—a condition where a system produces erroneous output without the internal state necessary to recognize the failure. `ail` addresses this by externalizing the monitor into a discrete, stateless execution step.
* **Statelessness is an advantage.** A fresh invocation (a self-evaluation step) cannot be contaminated by the confidence of the step that produced the output it's evaluating.

---

## It Isn't You

It is a Tuesday morning. You've poured your coffee, read the front page of Hacker News and opened a few new tabs. You're definitely going to read at least half of them later. Assuming the day doesn't get away from you that is.

There's thirty minutes before stand-up. You open the agent. You do that every day now. The C-suite is beyond sold on this stuff. You start to recognize the AI slop showing up on slide decks at -wide meetings. You smirk when the PowerPoint slide shows "It's not X, it's Y" in 28 point font to the five hundred people on the webcast.

You start running the prompt from yesterday's plan.

Some days this giant probability machine feels like magic. Every day, scattered across the half dozen articles you plan to read and the other dozen that your colleagues send to you are these breathless pieces of the coming wave that is the tool.

According to LinkedIn, everyone is about to be out of a career in six months. Also more valuable than ever. One guy says he runs Claude overnight and just reviews pull requests all day. Boris Cherny apparently runs up to ten agents in parallel (Cherny, 2026) and ships up to 100 PRs a week (Dunlop, 2026). One thing's for sure, he doesn't have to worry about the cost of tokens.

The results of your first prompt have come in. They're a little off, but there's an interesting thread you want to pull on. Maybe you should rethink the way you're going to tackle this problem. You type a question in a conversational tone, like you're talking to a colleague. Two architectural approaches, and you want the trade-offs before you commit. You hit enter.

A refactor has started tearing through your codebase like a forest fire. No no no... stop. Don't make changes until you present a plan and I confirm. You kick yourself. You know about plan mode. You let your guard down and it came back to bite you. You make a note that if you ever meet the person who invented "undo" that you're going to buy them a very fancy cup of coffee.

You start a fresh session.

Blips happen. That plan mode thing was your fault anyways. You bet that Boris never forgets to turn on plan mode. Maybe that's what one of his five parallel agents does. If only you had the time to learn this thing properly. Nobody puts "spend six months learning a tool that's going to be obsolete in four months" on their roadmap.

Later in the day you sit down to review the PR you're about to put your name on. It doesn't matter that the tool wrote it — your reputation is on the line for allowing it through. You know the failure modes by now. You know what to look for.

The code seems to do the job, it's hard to read in places, there are way too many tests (something you never thought you'd hear yourself say) and the same block of fifteen lines has been passed down the controller five times with just a single variable changed. The agent appears to have written each one from scratch, completely unaware of the others. You write a prompt to do a refactor and try to be a bit more DRY. You grumble to a colleague about how this happens every damned time. They commiserate and let you know that they saw Claude's system prompt has an instruction that makes that the default style. You're probably not beating that one with an update to your `memory.md`.  

You add it to the list of things you're definitely going to figure out when someone puts six months of "figuring stuff out" on the roadmap. The list is starting to get pretty long.

You push. CI fails. A linter violation and a failing test, both introduced in the last session. Both things a one-minute post-processing step would have caught. Didn't you literally say "ALWAYS" run the linter yesterday? You used caps and an exclamation mark and everything. Several exclamation marks, even. The robot can't tell that you're yelling at it.

There's a thread of dubious authority on social media where they say if you have to tell the LLM to do the same thing twice, you need to update your `memory.md`, or was it `Claude.md`? That you should spin up a background agent just to do code review. Someone else says that their agents are all talking to each other. They're doing standups in something that looks like Animal Crossing. This can't possibly be serious. You need to watch a two-hour YouTube video of some guy telling you how Boris does it.

Is this a me problem or is this a problem with the tool? Why is it on you to update memory every time? Shouldn't the tool be doing that anyways? They say the models are getting better every day.

Stop.

You are not frustrated with the tool exactly. The tool does remarkable things. You are frustrated with the gap — the repeatable, nameable gap between what the agent produces and what you can actually ship — and with the fact that you cannot quite tell whether closing that gap is your job or the tool's.

The failures aren't consistent enough to prove it's the tool. They're just consistent enough to make you wonder if it's you.

It isn't you.

## Your Agent Has a Diagnosis

Cognitive science has been studying them since Harlow published _Passage of an Iron Bar through the Head_ in an 1848 medical journal. Luria spent a career cataloguing the precise failure profile: perseveration, the collapse of goal-directed action, the inability to compare what you intended with what you actually did (Luria, 1966; Anokhin, 1955). By 1986, Baddeley had a name for the whole cluster of symptoms: Dysexecutive Syndrome. You have been independently rediscovering it ever since you typed your first prompt.

The treatment is executive function scaffolding (Diamond, 2013; Vygotsky, 1978). The best developers are already doing it. Every carefully structured CLAUDE.md, every skill that lays out a detailed plan for how to break down a task, every time you run plan mode before touching the codebase — that is executive function scaffolding, built by hand, from experience, one prompt at a time. The knowledge exists. It just isn't portable.

That is the actual gap. Every team builds this from scratch. Every new hire re-learns it. Every departure takes it with them.

`ail` is a specification for that layer — one that stays. A YAML file that lives in the repo, readable by any architect, inheritable from the organization or the wider industry, runnable without requiring someone to have seen the failures first. The executive function layer, finally written down.

The rest of this piece is the proof.

## Contents

1. The Problem A Prompt Can't Solve
2. The Dysexecutive Patient: A Taxonomy Borrowed from Neuroscience
3. What Forty Years of Cognitive Science Actually Knows About Deliberate Control
4. Build the Frontal Lobe

---

## The Problem A Prompt Can't Solve

Here is what is actually happening inside your agent session. Not every token in the context window is equal. Liu et al. established this in 2024 with a finding so consistent it now has a name — the lost-in-the-middle effect. Performance is highest when relevant information sits at the very beginning or end of the context. It degrades significantly when information sits in the middle, with accuracy drops exceeding 30% in controlled tests. This is a fundamental property of how attention works. It's in the math. The Chroma Research team confirmed it in 2025 across eighteen frontier models including GPT-4.1, Claude Opus 4, and Gemini 2.5 — finding degradation at every context length increment, beginning at the shortest tested inputs. A million-token context window rots early. The problem is noise accumulation.

To understand the mechanism in action it helps to understand the context lifecycle. It has a series of predictable behaviours over the course of a session. It starts clean. The system prompt is loaded. A list of available tools and MCP servers gets appended. Skill descriptions are included, with the full copy of any active skills added after that. Only then does your prompt get included. Then the session grows. Your LLM requests tool calls. File reads accumulate. Failed attempts and dead ends leave traces.

At one time, Claude Code's system prompt carried this explicitly (EliFuzz, 2025):

```markdown
VERY IMPORTANT: When you have completed a task, you MUST run the lint and typecheck commands (eg. npm run lint, npm run typecheck, ruff, etc.)
```

Whether it still does is beside the point. When Claude isn't working in a Node.js codebase it has to work harder to know which command to run — and what's more, as the session continues the context grows around that instruction. Eventually what was once a prominent signal is one small voice inside an enormous window — a very important needle in a haystack. Hong et al. tested this directly in 2025, evaluating eighteen models including GPT-4.1, Claude 4, and Gemini 2.5, and found that performance degrades consistently as input length increases, regardless of context window size. Noise accumulation begins at the shortest tested inputs. Zhu et al. confirmed in 2025 that the degradation cascades: a single early failure propagates through subsequent decisions, with memory and reflection errors particularly prone to compounding. The contaminated context works against recovery. This is why experienced developers say start a new session when something goes wrong. It is the correct response to a documented phenomenon.

The most common response to this problem has been to write more. Continually refining CLAUDE.md every time the agent fails. Writing skills that encode in ever more meticulous detail what has been missed. Sweller's cognitive load research tells us why this compounds the problem rather than solving it (Sweller, 1988). Every token added to a failing context increases extraneous load — the noise introduced by the system itself. The research on cognitive load predicts this will make things worse, not better.

The skills ecosystem is a genuine attempt to solve this — distribute the instructions, keep each skill focused, load only what's relevant. But a skill loaded into a contaminated context is subject to the same degradation as everything else in that context. The cure is made of the same stuff as the disease.

The industry's solution has been to build mitigation. The thousands of community skills, the carefully maintained local context files, the parallel agent architectures — these are intelligent responses to a real problem. The use of parallel and sub-agents appears to be effective because each agent has a less polluted context. Each parallel invocation is, functionally, a context reset. The field is finding the answer through instinct, but has left it unnamed.

`ail` moves the instruction out of the noise entirely: the linter runs as a declared step in a pipeline.

```yaml
version: "0.1"

pipeline:
  - id: invocation
    prompt: "{{ step.invocation.prompt }}"

  - id: run_lint
    context:
      shell: "cargo clippy -- -D warnings"
    on_result:
      - exit_code: 0
        action: break  # no errors — exit cleanly
      - exit_code: any
        action: continue

  - id: fix_lint
    prompt: |
      Fix the following linter errors:
      {{ step.run_lint.result }}
```

The linter runs after every invocation. If it passes, the pipeline exits cleanly. If it fails, the errors are passed directly to the model with an explicit fix instruction — not as a hope buried in a system prompt, but as the next step in a declared sequence. The context for `fix_lint` contains exactly what it needs: the linter output, nothing else.

There is a compounding benefit. An instruction in the pipeline frees its slot in the system prompt. The lost-in-the-middle effect applies to your own instructions too. A shorter, cleaner system prompt is a higher signal-to-noise system prompt. `ail` guarantees the behavior and it returns the attention budget to the work that actually requires language.

Your agent is behaving exactly as a system without executive control behaves. Cognitive scientists have been studying that system for over a century. They have a name for what you are seeing.

---

## The Dysexecutive Patient: A Taxonomy Borrowed from Neuroscience

In 1966, Alexander Luria published *Higher Cortical Functions in Man*, a systematic study of patients with damage to the prefrontal cortex. He described a coherent failure profile: patients who could hold a conversation, pass basic intelligence tests, and demonstrate perfectly preserved sensory and motor function — and yet would draw the same shape compulsively when asked to draw a series, would plane a board through to the bench underneath when the task was done. He called the underlying mechanism pathological inertia: the domination of behavior by the traces of previous actions, at the expense of current intention.

The failure profile had a name before the decade was out. By 1986, Alan Baddeley had synthesized the clinical literature under the term Dysexecutive Syndrome. A loss of the mechanism that intelligence requires to operate reliably: the ability to inhibit wrong responses, update working memory, shift between strategies, and compare what you intended to do with what you actually did.

Spend a day working with your agent. Then read Luria.

The failures have four names.

### Science already has names for this

**Perseveration.** The agent repeats the same action regardless of whether it worked. Luria's patients, asked to draw a circle and then a cross, would draw circles indefinitely. The LLM ecosystem has a partial answer: a tool call limit. The counter stops the loop. What it cannot tell you is whether anything in the loop was working.

**Goal substitution.** Norman and Shallice gave this its modern name in 1986, but Luria documented the phenomenon twenty years earlier. He described it as the disintegration of selective, goal-directed activity — a patient, asked to cook dinner, began stirring a pot that contained fiber instead of pasta. Your agent, asked whether to handle state in the database or in memory, begins writing the database implementation. You asked for a discussion. It got to work.

**Source monitoring failure.** Johnson, Hashtroudi and Lindsay characterized this in 1993: the inability to distinguish between information that was retrieved from memory and information that was generated. Hallucination is source monitoring failure — the model cannot tell the difference between what it read and what it produced. More practically: the agent that confidently cites a function that does not exist, the agent that remembers you having given an instruction that you did not give, the agent that treats its own intermediate reasoning as ground truth. The information is real to the model. Its origin is not checked.

**Anosognosia.** Babinski named this in 1914: the condition in which a patient with a neurological deficit has no awareness of the deficit. Luria documented it specifically in frontal patients — incorrect output produced, no problem reported, no internal signal that anything had gone wrong. Flavell formalized the broader cognitive principle in 1979 as metacognitive monitoring failure — the absence of the capacity to know what you don't know. Your agent always seems to have the right answer until you ask it if it was wrong. High confidence is a syntactic property of the output. A hallucinated citation and a real one look identical. The system cannot tell you it is uncertain because it has no mechanism for comparing its output against truth.

### The failures aren't random

The complexity of the attention mechanism and the hidden layers of neural networks makes these failures feel random. A century of clinical research says otherwise. They are the predictable behavioral profile of a system with capable reasoning and absent executive control. The same research that named them also knows how to treat them.

With a taxonomy naming the failures, they become addressable. A system with no inhibitory control mechanism can be given one. A system with no mechanism for comparing what it intended to do with what it actually did can be given that too. The table below maps each failure mode to the cognitive deficit that produces it, and to the `ail` primitive that addresses it. Every entry in the right column is executable YAML from the current spec.

| Failure Mode | Clinical Name | Cognitive Deficit | `ail` Structural Remedy |
|---|---|---|---|
| Agent repeats the same tool call | Perseveration | Inhibitory control failure | `max_retries:` + `on_error: abort_pipeline` |
| Agent acts on the wrong goal | Goal Substitution | Working memory updating failure | Explicit success criteria in `on_result:` + `break` vs `abort_pipeline` |
| Agent hallucinates earlier context | Source Monitoring Failure | Reality discrimination failure | Pipeline run log as authoritative state; `{{ step.<id>.response }}` |
| Agent reports success on bad output | Anosognosia | Metacognitive monitoring failure | Dedicated self-evaluation `prompt:` step with stateless fresh invocation |

The fourth component in Baddeley's list — the comparison of intention to outcome — is what Anokhin called the "action acceptor" in 1955, and what Luria returned to repeatedly as the key mechanism disrupted in frontal patients. Anokhin described it as a feedback circuit embedded in every voluntary action: programs for intended behavior are continuously compared against signals of actual effect. When the comparison detects a match, the action terminates. When it detects a mismatch, the action is prolonged or modified. When the mechanism is damaged, neither happens — the plan may form correctly, but the comparison never fires. Output is produced. No problem is registered.

Every pipeline step in `ail` is an action acceptor. The `on_result:` block is the comparison circuit. `break` is the termination signal on match. `pause_for_human` is the correction signal on mismatch. The pipeline run log is what makes the comparison possible: the intended prompt and the actual response are both persisted, independently, before the acceptor step runs — the mechanism cannot be contaminated by the output it is evaluating.

```yaml
version: "0.1"

pipeline:
  - id: invocation
    prompt: "{{ step.invocation.prompt }}"

  - id: action_acceptor
    prompt: |
      Original request: {{ step.invocation.prompt }}
      Result produced: {{ step.invocation.response }}
      Does the result achieve what was requested?
      Answer ACHIEVED or MISMATCH. One word only.
    on_result:
      contains: "ACHIEVED"
      if_true:
        action: break
      if_false:
        action: pause_for_human
        message: "Action acceptor detected a mismatch between intent and output. Review before continuing"
```

The field has been experiencing these failure modes since the first agent went to production. The clinical names are a diagnosis. And like any diagnosis, they point toward a treatment.

---

## What Forty Years of Cognitive Science Actually Knows About Deliberate Control

Cognitive science has had the treatment for forty years.

Adele Diamond's 2013 synthesis in the Annual Review of Psychology is the definitive modern integration of executive function research — a review of decades of behavioral, neuroimaging, and developmental evidence converging on a three-factor model: inhibitory control, working memory updating, and cognitive flexibility. Diamond wrote it for cognitive scientists. The mapping to agent failure modes is not a direct claim of the work, but is something that the evidence makes available — it is the treatment the previous section's diagnosis implies.

Each is a distinct component. Each can be developed independently. The prefrontal cortex and the limbic system work in concert — but there is a fundamental division of labour between them. Planning, inhibition, flexibility: these belong to the prefrontal layer. The limbic system was never built to supply them.

The history of AI so far is the history of building the limbic system. McCulloch and Pitts gave us the artificial neuron in 1943 — a unit that fires on a condition, modelled directly on biological nerve cells. Rumelhart, Hinton, and Williams gave us backpropagation in 1986: networks of those units, learning from examples, adjusting their weights toward better predictions. Vaswani and colleagues gave us the "Attention is All You Need" in 2017, and made large language models possible. Each step in that lineage produced a more capable system for pattern recognition, retrieval, and generation.

MacLean's triune brain model (1990) — reptilian brainstem, limbic system, neocortex, each layer evolving on top of the last — has been contested as neuroscience (Cesario et al., 2020). As a metaphor for what happened in AI, it is exact. The field built the pattern recognition layer. Then it built a better one. Then it built a transformer. Each iteration more capable, each one still operating without a supervisory layer above it. The gap wasn't visible until the capability beneath it was sophisticated enough to make it visible. You cannot recognise the need for a prefrontal layer until the system below it is capable enough to expose the absence. Now it is. The question that animated seventy years of research — can a machine learn to represent and generate knowledge — has been answered. The question it left behind is the harder one.

Diamond's three-factor model is the answer to that question. Each factor maps precisely to a failure mode from Section II — and each has a structural primitive in the `ail` spec.

**Inhibitory control** is the capacity to suppress dominant but incorrect responses. Luria's patients, asked to draw a cross after a circle, would draw another circle. The motor function was intact. The language comprehension was intact. The inhibitory mechanism was not. An agent that retries the same failing tool call for the eleventh time has intact pattern matching and no inhibitory primitive. `max_retries:` is the same as a tool call limit; combined with `on_error: <error-recovery-pipeline>`, it becomes an inhibitory control mechanism — the system stops the failing approach and hands off to a fresh context as declared behavior. `break` — the intentional exit — is the termination signal on successful match. The pattern is doing real cognitive work: the system has to know whether it stopped because it was done or because it failed.

**Working memory updating** is the capacity to hold task-relevant information across steps, incorporate new information, and release information that is no longer relevant. Baddeley's 1974 architecture is useful here: the central executive does not store information. It selects which information is active. The context window stores everything. Selecting from it is a different function than holding it — and the selection mechanism is what goes missing in agent architectures that treat the context window as working memory. The pipeline run log, accessed via `{{ step.<id>.response }}`, is the selection mechanism. A dedicated step can distill the pipeline run log into exactly what the next step needs — the relevant thread of the conversation, extracted and re-presented, leaving the full history intact in the log without burdening the active context with it.

**Cognitive flexibility** is the capacity to shift — between tasks, between rules, between frames when the current one stops working. Duncker's subjects in the candle problem had intact reasoning and intact knowledge of every object on the table. What they couldn't do was reframe the box. It remained a container when the task needed a shelf. Luria's frontal patients showed the same failure pattern at a coarser grain: asked to switch from circles to crosses, they drew more circles. The capacity to abandon a working approach when it stops working is distinct from the capacity to execute it. `on_result:` branches when output meets criteria; `on_error:` routes to a different strategy when execution fails; a conditional `pipeline:` call replaces the active system prompt entirely when the task changes. The agent writing tests doesn't need the instructions for writing features. Cognitive flexibility, in practice, is deliberate situational forgetting.

Diamond is explicit about the interdependence. Inhibitory control enables working memory updating by clearing the noise that would otherwise contaminate the selection buffer. Cognitive flexibility requires both — to shift perspective, you must first inhibit the current one and load a different frame into working memory. Remove any one factor and the system degrades. Remove all three and what remains is a capable reasoner with no mechanism for governing its own reasoning.

### Intelligence Is Knowing Which Script to Run

In Schank and Abelson's account, intelligence is largely the capacity to recognize which existing knowledge structure applies and to activate it.

Understanding a situation does not require reasoning from first principles. It requires recognizing which stored event structure applies and instantiating it. These scripts are "predetermined, stereotyped sequence[s] of actions that define a well-known situation," made up of slots and requirements about what can fill those slots. The restaurant script. The doctor's visit. The code review. In their account, expertise is, in significant part, a well-stocked library of scripts and the capacity to match situations to them accurately. Recognition, then instantiation.

The activation mechanism matters. A script fires when its context headers are recognized: a locale, a precondition, an instrumental cue. Once activated, the script supplies the expected event sequence and fills in what wasn't stated. The gaps in a story about going to a restaurant don't read as gaps — the script closes them. The "fancy restaurant track" and the "fast food track" are sub-paths within the same structure, each with its own slot-fillers, each instantiated based on which cues appear in context.

An experienced developer reviewing a pull request is not reasoning from first principles. The script has activated — security patterns, test coverage, naming conventions, blast radius estimation. It fires on context recognition.

With syntax borrowed directly from Docker's `FROM`, `ail` takes the capability further. Docker's `FROM` allows you to build on top of the base image. `ail` gives you the base pipeline and lets you reach into it — hooking `before:` or `after:` named steps, overriding specific ones, disabling what the domain no longer needs. The payments team uses the base script that is shared across the organization, and adds a PCI check `before: code_review`. That is script instantiation in Schank's precise sense.

The mechanism visible:

```yaml
FROM: ./vendor/org-base.yaml

pipeline:
  - run_before: code_review
    id: pci_compliance_check
    skill: ./skills/pci-checker/
```

The `FROM:` is the script activation. The `run_before:` is the slot being filled. The base pipeline is inherited unchanged. The instantiation supplies only what the domain requires.

### Models Are Getting Better

Diamond's three factors are properties of architecture. A larger declarative memory does not produce inhibitory control. A more capable pattern-matching engine does not produce working memory updating. Extended pretraining does not yield cognitive flexibility as an emergent property — they are distinct cognitive components. The experimental literature is unambiguous: they do not reduce to each other. They do not emerge from capability in adjacent domains.

---


## Build the Frontal Lobe

The artificial neuron exists because McCulloch and Pitts sat down in 1943 to model the biological one. Backpropagation exists because Rumelhart's group was trying to explain how humans learn. The transformer is built around attention — a psychological construct before it is a mathematical one. In the preface to *Parallel Distributed Processing*, Rumelhart described a team that included physicists, neuroscientists, molecular biologists, and computer scientists alongside psychologists, and hoped the work would be read the same way it was written: across disciplines.

The same tradition that supplied the concepts documented what they couldn't provide. Baddeley published the central executive in 1974. Diamond synthesized the executive function research in 2013. The gaps were described, named, and studied — in the same literature that gave the field its foundations. The treatment was written before the patient existed.

`ail` is the hypothesis that the treatment fits in a YAML file. A declared pipeline that runs before the human sees the output. Inhibitory control as `max_retries:`. Working memory as a persisted run log. Cognitive flexibility as `on_result:` branches that replace the active system prompt when the task changes. A `FROM:` chain that carries accumulated domain knowledge across sessions and across teams. The executive function layer, assembled from primitives, version-controlled, and deployable today.

The models are finding the same gaps on their own. In March 2026, MiniMax ran M2.7 through over 100 autonomous rounds of modifying its own scaffold code, evaluating results, and deciding whether to keep or revert changes with no human involvement. The system achieved a 30% performance improvement on internal benchmarks. M2.7 discovered mid-loop that it needed an inhibitory control mechanism and built one. When the spec matures to the point that agents can write and modify their own pipelines — writing task-specific prompts per step — the next step change in LLM performance may come in the form of a YAML file.

What ships now is a proof of concept. Here is what it points toward.

Every team that runs agents long enough builds some version of this layer. A CLAUDE.md that grows by one paragraph every time the agent does something unexpected. A wrapper script that runs the linter after every session. A checklist the senior engineer mentally applies before approving the PR. The human in the loop of our operating model is an undeclared, non-transferable executive function layer getting built from scratch by every developer and team in isolation.

We can build this together.

### What We're Building

The instrument has to work before the interesting questions can run. Two steps: finalize the spec to incorporate the pending design decisions — the pipeline language redesign, the context member model, the composable system prompt — and complete a working runner interface with a terminal UI that feels like using the tool rather than configuring it. The target is a Claude Code-style console: stream the output, run the pipeline, show the human what happened. Fast enough that running it is the obvious choice. Unremarkable enough that it disappears.

What exists today is a proof of concept: a YAML file successfully driving two prompts in sequence. The roadmap for the next six months focuses on moving from this foundation to a production-grade instrument—building out the handlers, proving performance claims, and developing a stable terminal interface.

### The Empirical Test

The hypothesis is specific enough to be wrong — and the field is already running early versions of the experiment.

Karpathy's autoresearch gives an agent a training codebase and a `program.md` — what he explicitly calls "a super lightweight skill" — and lets it run autonomously: micro-changes, each small enough to evaluate in five minutes, dozens of attempts per hour, hundreds overnight. MiniMax did the same with agent scaffold code — the operating procedures, sampling parameters, and workflow guidelines that direct the model without touching its weights: 100+ autonomous rounds of modification, a 30% performance improvement, no human checkpoints. Both arrived at the same architectural instinct: a self-improving loop directed by a lightweight specification file. The active science is making the case. What `ail` proposes is that formalizing that instinct — declaring the pipeline, version-controlling it, making it human-reviewable — is where the next wave of improvement comes from.

SWE-bench measures whether a model can resolve real GitHub issues against real codebases. Every frontier lab publishes scores produced by the model alone, or with scaffolding the lab designed. The question `ail` is positioned to answer is concrete and runnable: can a set of declared pipelines — linter, test runner, action acceptor, self-evaluation step — improve a model's own published score using that same model, with no changes to the weights?

The evidence suggests they can. SWE-bench failures cluster around exactly the failure modes the previous sections named. The model produces output. The output is not verified against the test suite. There is no comparison circuit. The same model, running against a pipeline that guarantees the linter passes and the tests run before the score is recorded, should do better. The executive layer is doing measurable work, or it is not.

Either outcome is useful. A confirmed improvement is evidence that the architectural claim is correct. A null result is a prompt to revise the spec. The benchmark exists. The pipelines can be written. If you work in model evaluation — at a frontier lab or elsewhere — this is an invitation to design the experiment together.

### Getting Better On Purpose

With a human in the loop, the agent has a collaborator. The pipeline accumulates evidence. The human decides what to keep and shapes the definition of done.

The pipeline run log accumulates. Every invocation: the prompt, the response, the tool calls, the `on_result` branch that fired, the step that handed off to human review. Over a session. Over a project. Over months of a team working in the same repository.

That log is a structured record of how the agent fails and how those failures were resolved. The next step is the pipeline that reads it:

```yaml
# Note: log-injection and hot-reload require planned primitives (D-019)
# The architecture below reflects the design trajectory, not the current spec.
# You can track the progress of this in the at https://github.com/AlexChesser/ail

- id: pipeline_reflection
  prompt: |
    Review the attached pipeline run log.
    Identify the most common mismatch between intended and actual output.
    Propose a new step that would prevent it.
    Format your response as a YAML diff targeting the existing pipeline.
  on_result:
    always:
      action: pause_for_human
      message: "Pipeline improvement proposed. Approve to apply."
```

On approval, the diff is committed. The pipeline hot-reloads. The next invocation runs against an improved version of itself.

Every primitive this requires is in the current spec or the active decisions queue. `pause_for_human` gates human approval. `FROM:` applies the improvement as a new inheriting layer, leaving the base unchanged and the change auditable. The pipeline accumulates the operational knowledge of the team, expressed in a format that is readable, diffable, and testable. Every fresh session inherits the distilled output of every previous one.

### One Step Further

The pipeline that does not wait for the session to end.

Fan-out within a single invocation — the same prompt, two runners, independent models and contexts — two responses that can be compared by a third step that was designed for nothing else. That comparison step can identify what the better response did differently. It can propose the prompt modification that would have produced the better result from the weaker runner. It can write that modification to the pipeline before the human sees the output.

```yaml
# Note: parallel execution requires planned primitives (D-020)

- id: implement_a
  prompt: "{{ step.invocation.prompt }}"
  provider: frontier

- id: implement_b
  prompt: "{{ step.invocation.prompt }}"
  provider: commodity

- id: compare
  prompt: |
    Two implementations of the same task:
    A: {{ step.implement_a.response }}
    B: {{ step.implement_b.response }}
    What did A do better? What prompt change would produce A's result from B's model?
  on_result:
    always:
      action: pause_for_human
      message: "Quality comparison complete. Approve pipeline update?"
```

A feedback loop that improves on multiple vectors simultaneously — routing to cheaper models where quality holds, tuning temperature upward for steps that reward creative output and back down when the task shifts to deterministic code, tightening instructions where the weaker runner consistently diverges. Systematically, from evidence, within the course of a single session. The gap between frontier and commodity closes because the pipeline's instructions get better at directing it.

Run long enough, this becomes a mechanism for encoding what good software looks like into an artifact that is executable, inheritable, and self-improving. The accumulated operational intelligence of every developer who has ever committed to that repository lives in a YAML file that any agent can run and any engineer can read. The pipeline becomes the moat. Any team can access the same models. Only your team has the years of evidence baked into how you direct them.

### What Comes Next

We can see the artificial brain taking shape. The cerebellum is there. The limbic system is there. And now, in real time, in the active science of self-improving loops running without human checkpoints, we are watching emergent behaviour reach for a frontal lobe. Systems building their own mechanisms to decide when to stop, when to ask, when to reframe, when to reject their own output. Feeling their way toward an executive layer that nobody gave them — because nobody knew to.

A century of cognitive science has a name for what they are looking for.

`ail` is our attempt to hand it to them.

The open question is whether the executive layer gets built deliberately — declared, version-controlled, human-reviewable, inheritable — or whether it keeps getting rediscovered from scratch by every developer who hits the failure modes first and understands them second. Whether it sits as a shared layer that benefits any model that needs it, or gets built inside a frontier lab and held as a proprietary moat. Whether the incentive structure rewards the people doing the work or the capital behind them.

Every pipeline contributed to the ecosystem is a piece of operational knowledge that survives beyond the session that produced it. Every run that confirms or contradicts the SWE-bench hypothesis moves the field forward. 

On a long enough timeline, these systems get built.

---

## Post-Script: A Request for Stars

This project is a labor of love, currently balanced against a full-time career and a young family. Because developer time is the most finite resource in this equation, the project’s velocity depends entirely on community signal.

If this thesis resonated with you, the most impactful thing you can do is star the repository at https://github.com/AlexChesser/ail.

High engagement levels qualify `ail` for specialized open-source support programs, including compute grants and frontier model access. These resources provide the primary fuel for this development; they are exactly what is required to benchmark these pipelines against the SWE-bench Pro dataset and get a working, empirically validated tool into your hands faster.

The kids stay at number one; your support helps everything else find its place. I need your stars, ironically, because attention really is all you need.

---

## References

Anokhin, P. K. (1955). Features of the afferent apparatus of the conditioned reflex and their importance for psychology. *Voprosy Psikhologii, 6*, 16–38. [In Russian; English summary in Anokhin, P. K. (1974). *Biology and neurophysiology of the conditioned reflex and its role in adaptive behavior.* Pergamon Press.]

Babinski, J. (1914/2014). Contribution to the study of the mental disorders in hemiplegia of organic cerebral origin (anosognosia) (K. G. Langer & D. N. Levine, Trans.). Cortex, 61, 5–8. https://doi.org/10.1016/j.cortex.2014.04.019 (Original work published 1914)

Baddeley, A. D. (1986). *Working memory.* Oxford University Press.

Baddeley, A.D. & Hitch, G. (1974). Working memory. In G.H. Bower (Ed.), *Psychology of Learning and Motivation* (Vol. 8, pp. 47–89). Academic Press.

Cesario, J., Johnson, D. J., & Eisthen, H. L. (2020). Your brain is not an onion with a tiny reptile inside. *Current Directions in Psychological Science, 29*(3), 255–260.

Cherny, B. (2026). https://x.com/bcherny/status/2007179832300581177

Diamond, A. (2013). Executive functions. *Annual Review of Psychology, 64*, 135–168.

Dunlop, A. (2026). Claude Code’s Creator, 100 PRs a Week — His Setup Will Surprise You, https://medium.com/vibe-coding/claude-codes-creator-100-prs-a-week-his-setup-will-surprise-you-7d6939c99f2b

EliFuzz. (2025, July 23). Claude Code system prompt [Leaked system prompt, archived]. GitHub. https://github.com/EliFuzz/awesome-system-prompts/blob/main/leaks/anthropic/archived/2025-07-23_prompt_system.js#L106

Flavell, J.H. (1979). Metacognition and cognitive monitoring: A new area of cognitive-developmental inquiry. *American Psychologist, 34*(10), 906–911.

Harlow, J. M. (1848). Passage of an iron rod through the head. *Boston Medical and Surgical Journal, 39*(20), 389–393.

Hong, K., & Chroma Research Team. (2025). Context rot: How increasing input tokens impacts LLM performance. Chroma Research. https://research.trychroma.com/context-rot

Johnson, M.K., Hashtroudi, S., & Lindsay, D.S. (1993). Source monitoring. *Psychological Bulletin, 114*(1), 3–28.

Karpathy, A. (2026). autoresearch: AI agents running research on single-GPU nanochat training automatically [Software]. GitHub. https://github.com/karpathy/autoresearch

Liu, N. F., Lin, K., Hewitt, J., Paranjape, A., Bevilacqua, M., Petroni, F., & Liang, P. (2024). Lost in the middle: How language models use long contexts. Transactions of the Association for Computational Linguistics, 12, 157–173. https://doi.org/10.1162/tacl_a_00638

Luria, A.R. (1966). *Higher cortical functions in man.* Basic Books.

MacLean, P. D. (1990). *The triune brain in evolution: Role in paleocerebral functions.* Plenum Press.

McCulloch, W. S., & Pitts, W. (1943). A logical calculus of the ideas immanent in nervous activity. *Bulletin of Mathematical Biophysics, 5*(4), 115–133.

MiniMax. (2026, March 18). MiniMax M2.7: Early echoes of self-evolution. MiniMax. https://www.minimax.io/news/minimax-m27-en

Norman, D. A., & Shallice, T. (1986). Attention to action: Willed and automatic control of behavior. In R. J. Davidson, G. E. Schwartz, & D. E. Shapiro (Eds.), Consciousness and self-regulation: Advances in research and theory (Vol. 4, pp. 1–18). Plenum Press.

Rumelhart, D. E., Hinton, G. E. & Williams, R. J. in *Parallel Distributed Processing: Explorations in the Microstructure of Cognition. Vol. 1: Foundations* (eds Rumelhart, D. E. & McClelland, J. L.) 318–362 (MIT, Cambridge, 1986).

Rumelhart, D., Hinton, G. & Williams, R. Learning representations by back-propagating errors. Nature 323, 533–536 (1986). https://doi.org/10.1038/323533a0

Schank, R.C. & Abelson, R.P. (1977). *Scripts, plans, goals and understanding.* Lawrence Erlbaum.

Sweller, J. (1988). Cognitive load during problem solving: Effects on learning. *Cognitive Science, 12*(2), 257–285.

Vaswani, A., Shazeer, N., Parmar, N., Uszkoreit, J., Jones, L., Gomez, A. N., Kaiser, Ł., & Polosukhin, I. (2017). Attention is all you need. *Advances in Neural Information Processing Systems, 30*, 5998–6008.

Vygotsky, L.S. (1978). *Mind in society: The development of higher psychological processes.* Harvard University Press.

Zhu, K., Liu, Z., Li, B., Tian, M., Yang, Y., Zhang, J., Han, P., Xie, Q., Cui, F., Zhang, W., Ma, X., Yu, X., Ramesh, G., Wu, J., Liu, Z., Lu, P., Zou, J., & You, J. (2025). Where LLM agents fail and how they can learn from failures. arXiv:2509.25370.
