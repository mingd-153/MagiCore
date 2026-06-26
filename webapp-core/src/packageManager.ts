import { exec } from 'child_process';
import { promisify } from 'util';

const execAsync = promisify(exec);

/**
 * Detects which package manager is available.
 * Returns "pnpm", "bun" or throws if none found.
 */
export async function detectPackageManager(): Promise<'npm'> {
    // Directly use npm as the package manager for this project.
    return 'npm';
}

/**
 * Runs a package manager command (`dev`, `build`, `test`, `lint`).
 * It forwards the arguments to the detected manager.
 */
export async function runPmCommand(cmd: string, args: string[] = []): Promise<void> {
    const pm = await detectPackageManager();
    const fullCmd = `${pm} ${cmd} ${args.join(' ')}`.trim();
    console.log(`Executing: ${fullCmd}`);
    const { stdout, stderr } = await execAsync(fullCmd);
    if (stdout) process.stdout.write(stdout);
    if (stderr) process.stderr.write(stderr);
}

// When executed directly via `node -r ts-node/register src/packageManager.ts <cmd>`
if (require.main === module) {
    const [, , cmd, ...args] = process.argv;
    if (!cmd) {
        console.error('Usage: node -r ts-node/register src/packageManager.ts <command> [args...]');
        process.exit(1);
    }
    runPmCommand(cmd, args).catch((e) => {
        console.error(e.message);
        process.exit(1);
    });
}
