import { runPmCommand } from './packageManager';

runPmCommand('dev').catch((e) => {
    console.error('Dev command failed:', e.message);
    process.exit(1);
});
