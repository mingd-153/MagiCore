// MGPM TypeScript CLI entry point
// Bundled with the napi-rs native addon

export * from './index';

// CLI entry
if (require.main === module) {
  const { MgpmCli } = require('./index');
  const cli = new MgpmCli();
  const args = process.argv.slice(2);

  async function main() {
    const cmd = args[0];

    try {
      switch (cmd) {
        case 'install': {
          const result = await cli.install({
            offline: args.includes('--offline') || args.includes('-o'),
            frozen_lockfile: args.includes('--frozen-lockfile'),
            production: args.includes('--production') || args.includes('-p'),
            dev: false,
            optional: false,
            dry_run: args.includes('--dry-run'),
            concurrency: 16,
          });
          console.log(result);
          break;
        }
        case 'add': {
          const pkgs = args.filter(a => !a.startsWith('-'));
          const result = await cli.add(pkgs[1], {
            dev: args.includes('--dev') || args.includes('-D'),
            optional: args.includes('--optional') || args.includes('-O'),
            peer: args.includes('--peer') || args.includes('-P'),
            exact: args.includes('--exact') || args.includes('-E'),
            save: true,
          });
          console.log(result);
          break;
        }
        case 'remove': {
          const pkg = args[1];
          const result = await cli.remove(pkg);
          console.log(result);
          break;
        }
        case 'update': {
          const result = await cli.update({
            latest: args.includes('--latest') || args.includes('-L'),
            save: true,
            dev: false,
          });
          console.log(result);
          break;
        }
        case 'run': {
          const script = args[1];
          const scriptArgs = args.slice(2);
          const result = await cli.run(script, scriptArgs);
          console.log(result);
          break;
        }
        case 'exec': {
          const command = args[1];
          const execArgs = args.slice(2);
          const result = await cli.exec(command, execArgs);
          console.log(result);
          break;
        }
        case 'store': {
          const sub = args[1];
          if (sub === 'prune') {
            const result = await cli.storePrune();
            console.log(result);
          } else if (sub === 'status') {
            const result = await cli.storeStatus();
            console.log(result);
          }
          break;
        }
        case 'config': {
          const sub = args[1];
          if (sub === 'get') {
            const result = await cli.configGet(args[2]);
            console.log(result);
          } else if (sub === 'set') {
            const result = await cli.configSet(args[2], args[3]);
            console.log(result);
          } else if (sub === 'delete') {
            const result = await cli.configDelete(args[2]);
            console.log(result);
          } else if (sub === 'list') {
            const result = await cli.configList();
            console.log(result);
          }
          break;
        }
        case 'init': {
          const result = await cli.init();
          console.log(result);
          break;
        }
        default:
          console.log('MGPM - MegaGate Package Manager');
          console.log('Usage: mgpm <command> [options]');
          console.log('');
          console.log('Commands:');
          console.log('  install    Install dependencies');
          console.log('  add        Add a package');
          console.log('  remove     Remove a package');
          console.log('  update     Update packages');
          console.log('  run        Run a script');
          console.log('  exec       Execute a command');
          console.log('  store      Manage the store');
          console.log('  config     Manage configuration');
          console.log('  init       Initialize a project');
      }
    } catch (err: any) {
      console.error('Error:', err.message);
      process.exit(1);
    }
  }

  main();
}
