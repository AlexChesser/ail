"use strict";
/**
 * Binary resolution — finds the ail executable to use.
 *
 * Resolution order:
 *   1. ail.binaryPath setting (if set and file exists)
 *   2. Bundled binary in dist/ail-{platform-triple}
 *   3. ail on PATH
 *
 * Reports a warning (not an error) if the resolved binary's version is below
 * the minimum declared in package.json#config.ailMinVersion.
 */
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __setModuleDefault = (this && this.__setModuleDefault) || (Object.create ? (function(o, v) {
    Object.defineProperty(o, "default", { enumerable: true, value: v });
}) : function(o, v) {
    o["default"] = v;
});
var __importStar = (this && this.__importStar) || (function () {
    var ownKeys = function(o) {
        ownKeys = Object.getOwnPropertyNames || function (o) {
            var ar = [];
            for (var k in o) if (Object.prototype.hasOwnProperty.call(o, k)) ar[ar.length] = k;
            return ar;
        };
        return ownKeys(o);
    };
    return function (mod) {
        if (mod && mod.__esModule) return mod;
        var result = {};
        if (mod != null) for (var k = ownKeys(mod), i = 0; i < k.length; i++) if (k[i] !== "default") __createBinding(result, mod, k[i]);
        __setModuleDefault(result, mod);
        return result;
    };
})();
Object.defineProperty(exports, "__esModule", { value: true });
exports.platformTriple = platformTriple;
exports.resolveBinary = resolveBinary;
exports.clearBinaryCache = clearBinaryCache;
const fs = __importStar(require("fs"));
const path = __importStar(require("path"));
const child_process_1 = require("child_process");
const vscode = __importStar(require("vscode"));
let cached;
/** Returns the platform triple used for bundled binary naming. */
function platformTriple() {
    const arch = process.arch === "arm64" ? "aarch64" : "x86_64";
    const platformMap = {
        darwin: "apple-darwin",
        linux: "unknown-linux-musl",
        win32: "pc-windows-msvc",
    };
    const plat = platformMap[process.platform] ?? "unknown-linux-musl";
    return `${arch}-${plat}`;
}
/** Returns the bundled binary filename for this platform. */
function bundledBinaryName() {
    const triple = platformTriple();
    return process.platform === "win32" ? `ail-${triple}.exe` : `ail-${triple}`;
}
/** Run `ail --version` and return the version string. */
async function getVersion(binaryPath) {
    return new Promise((resolve, reject) => {
        (0, child_process_1.execFile)(binaryPath, ["--version"], { timeout: 5000 }, (err, stdout) => {
            if (err) {
                reject(err);
            }
            else {
                // Output format: "ail 0.1.0" — extract last token
                const version = stdout.trim().split(/\s+/).pop() ?? "unknown";
                resolve(version);
            }
        });
    });
}
/** Compare two semver strings. Returns true if actual >= minimum. */
function meetsMinVersion(actual, minimum) {
    const parse = (v) => v.split(".").map((n) => parseInt(n, 10) || 0);
    const [maj1, min1, pat1] = parse(actual);
    const [maj2, min2, pat2] = parse(minimum);
    if (maj1 !== maj2)
        return maj1 > maj2;
    if (min1 !== min2)
        return min1 > min2;
    return pat1 >= pat2;
}
/**
 * Resolve the ail binary. Caches the result for the lifetime of the extension.
 * Call this once during activation and pass the result around.
 */
async function resolveBinary(context) {
    if (cached) {
        return cached;
    }
    const config = vscode.workspace.getConfiguration("ail-chat");
    const configuredPath = config.get("binaryPath", "");
    let binaryPath;
    // 1. User-configured path
    if (configuredPath && fs.existsSync(configuredPath)) {
        binaryPath = configuredPath;
    }
    // 2. Bundled binary
    if (!binaryPath) {
        const bundled = path.join(context.extensionPath, "dist", bundledBinaryName());
        if (fs.existsSync(bundled)) {
            binaryPath = bundled;
            // Ensure executable
            try {
                fs.chmodSync(bundled, 0o755);
            }
            catch {
                // May already be executable; ignore
            }
        }
    }
    // 3. PATH fallback
    if (!binaryPath) {
        binaryPath = process.platform === "win32" ? "ail.exe" : "ail";
    }
    // Verify binary works and get version
    let version;
    try {
        version = await getVersion(binaryPath);
    }
    catch (err) {
        const msg = `ail binary not found or not executable at '${binaryPath}'. Set ail-chat.binaryPath to override.`;
        void vscode.window.showErrorMessage(msg);
        throw new Error(msg);
    }
    // Check minimum version
    const pkgJson = JSON.parse(fs.readFileSync(path.join(context.extensionPath, "package.json"), "utf-8"));
    const minVersion = pkgJson.config?.ailMinVersion ?? "0.0.0";
    if (!meetsMinVersion(version, minVersion)) {
        void vscode.window.showWarningMessage(`ail ${version} is below the minimum required version ${minVersion}. ` +
            `Some features may not work correctly. Update ail or set ail.binaryPath.`);
    }
    cached = { path: binaryPath, version };
    return cached;
}
/** Clear the cached binary (call on configuration change). */
function clearBinaryCache() {
    cached = undefined;
}
//# sourceMappingURL=binary.js.map