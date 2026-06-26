import { runPmCommand } from './packageManager';

runPmCommand('test').catch((e) => {
    console.error('Test failed:', e.message);
    process.exit(1);
});
