import { runPmCommand } from './packageManager';

runPmCommand('build').catch((e) => {
    console.error('Build failed:', e.message);
    process.exit(1);
});
