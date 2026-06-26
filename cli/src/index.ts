import { Command } from 'commander';
const program = new Command();
import { createInstaller } from './installer/rust.js';
import { getStoreDir } from './store.js';
import { InstallOptions } from './types.js';

const VERSION = '0.1.0';

function createOptions(): InstallOptions {
    return {
        registry: process.env.npm_config_registry || 'https://registry.npmjs.org',
        storeDir: getStoreDir(),
        production: false,
        frozenLockfile: false,
    };
}

program
    .name('megagate-pm')
    .description('MegaGate Package Manager - Fast, deterministic, content-addressable')
    .version(VERSION);

program
    .command('install')
    .description('Install dependencies from package.json')
    .option('--frozen-lockfile', 'Fail if lockfile is out of sync')
    .option('--production', 'Skip devDependencies')
    .option('--registry <url>', 'Registry URL')
    .option('--store-dir <path>', 'Custom store directory')
    .action(async (options: any) => {
        const opts = createOptions();
        if (options.frozenLockfile) opts.frozenLockfile = true;
        if (options.production) opts.production = true;
        if (options.registry) opts.registry = options.registry;
        if (options.storeDir) opts.storeDir = options.storeDir;

        const installer = createInstaller(opts);
        try {
            const result = await installer.install(opts);
            console.log(`✓ Installed ${result.added.length} packages`);
            process.exit(0);
        } catch (e: any) {
            console.error(`✗ Install failed: ${e.message}`);
            process.exit(1);
        }
    });

program
    .command('add <spec>')
    .description('Add a dependency')
    .option('-D, --dev', 'Add to devDependencies')
    .option('-O, --optional', 'Add to optionalDependencies')
    .option('--registry <url>', 'Registry URL')
    .option('--store-dir <path>', 'Custom store directory')
    .action(async (spec: string, options: any) => {
        const opts = createOptions();
        if (options.registry) opts.registry = options.registry;
        if (options.storeDir) opts.storeDir = options.storeDir;

        const installer = createInstaller(opts);
        try {
            const result = await installer.add(spec, {
                dev: options.dev,
                optional: options.optional,
            });
            console.log(`✓ Added ${result.added.join(', ')}`);
            process.exit(0);
        } catch (e: any) {
            console.error(`✗ Add failed: ${e.message}`);
            process.exit(1);
        }
    });

program
    .command('update [spec]')
    .description('Update dependencies')
    .option('--latest', 'Update to latest version ignoring range')
    .option('--registry <url>', 'Registry URL')
    .option('--store-dir <path>', 'Custom store directory')
    .action(async (spec: string | undefined, options: any) => {
        const opts = createOptions();
        if (options.registry) opts.registry = options.registry;
        if (options.storeDir) opts.storeDir = options.storeDir;

        const installer = createInstaller(opts);
        try {
            const result = await installer.update(spec, { latest: options.latest });
            console.log(`✓ Updated ${result.updated.length} packages`);
            process.exit(0);
        } catch (e: any) {
            console.error(`✗ Update failed: ${e.message}`);
            process.exit(1);
        }
    });

program
    .command('remove <name>')
    .description('Remove a dependency')
    .option('--store-dir <path>', 'Custom store directory')
    .action(async (name: string, options: any) => {
        const opts = createOptions();
        if (options.storeDir) opts.storeDir = options.storeDir;

        const installer = createInstaller(opts);
        try {
            await installer.remove(name);
            console.log(`✓ Removed ${name}`);
            process.exit(0);
        } catch (e: any) {
            console.error(`✗ Remove failed: ${e.message}`);
            process.exit(1);
        }
    });

program
    .command('list')
    .description('List installed packages')
    .option('-d, --depth <number>', 'Depth of tree', '0')
    .option('--json', 'Output as JSON')
    .option('--store-dir <path>', 'Custom store directory')
    .action(async (options: any) => {
        const opts = createOptions();
        if (options.storeDir) opts.storeDir = options.storeDir;

        const installer = createInstaller(opts);
        try {
            const deps = await installer.list(parseInt(options.depth, 10));
            if (options.json) {
                console.log(JSON.stringify(deps, null, 2));
            } else {
                for (const [name, version] of Object.entries(deps)) {
                    console.log(`${name}@${version}`);
                }
            }
            process.exit(0);
        } catch (e: any) {
            console.error(`✗ List failed: ${e.message}`);
            process.exit(1);
        }
    });

program
    .command('verify')
    .description('Verify lockfile integrity')
    .option('--store-dir <path>', 'Custom store directory')
    .action(async (options: any) => {
        const opts = createOptions();
        if (options.storeDir) opts.storeDir = options.storeDir;

        const installer = createInstaller(opts);
        try {
            const result = await installer.verify();
            if (result.valid) {
                console.log('✓ Lockfile integrity verified');
                process.exit(0);
            } else {
                console.error('✗ Integrity check failed:');
                for (const err of result.errors) {
                    console.error(`  - ${err}`);
                }
                process.exit(3);
            }
        } catch (e: any) {
            console.error(`✗ Verify failed: ${e.message}`);
            process.exit(1);
        }
    });

const storeCmd = program.command('store').description('Store management');

storeCmd
    .command('path')
    .description('Print store path')
    .action(() => {
        console.log(getStoreDir());
    });

storeCmd
    .command('prune')
    .description('Remove unreferenced packages from store')
    .action(async () => {
        console.log('Store prune not implemented yet');
        process.exit(0);
    });

storeCmd
    .command('verify')
    .description('Verify all packages in store')
    .action(async () => {
        console.log('Store verify not implemented yet');
        process.exit(0);
    });

program.parseAsync(process.argv).catch((e: Error) => {
    console.error(e.message);
    process.exit(1);
});
