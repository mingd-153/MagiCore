#!/usr/bin/env node
import { Command } from 'commander';
import { Mgpm } from '../index.js';

const program = new Command();
const mgpm = new Mgpm();

program
  .name('mgpm')
  .description('MegaGate Package Manager')
  .version('0.1.0');

program
  .command('install')
  .option('--offline', 'Install from cache only')
  .option('--frozen-lockfile', 'Do not update lockfile')
  .option('--production', 'Skip dev dependencies')
  .option('--hoist', 'Hoist packages to root node_modules')
  .action(async (opts) => {
    const result = await mgpm.install({
      offline: opts.offline,
      production: opts.production,
    });
    console.log(result);
  });

program
  .command('add <packages...>')
  .option('-D, --dev', 'Add as dev dependency')
  .option('-P, --peer', 'Add as peer dependency')
  .option('-O, --optional', 'Add as optional dependency')
  .option('-E, --exact', 'Save exact version')
  .action(async (packages, opts) => {
    for (const pkg of packages) {
      const result = await mgpm.add(pkg, {
        dev: opts.dev,
        peer: opts.peer,
        optional: opts.optional,
        exact: opts.exact,
      });
      console.log(result);
    }
  });

program
  .command('remove <packages...>')
  .action(async (packages) => {
    for (const pkg of packages) {
      const result = await mgpm.remove(pkg);
      console.log(result);
    }
  });

program
  .command('update [package]')
  .option('--latest', 'Update to latest version')
  .action(async (pkg, opts) => {
    const result = await mgpm.update({ latest: opts.latest });
    console.log(result);
  });

program
  .command('run <script>')
  .allowUnknownOption(true)
  .action(async (script, args) => {
    const result = await mgpm.run(script);
    console.log(result);
  });

program
  .command('exec <command>')
  .allowUnknownOption(true)
  .action(async (command, args) => {
    const result = await mgpm.exec(command);
    console.log(result);
  });

program
  .command('store')
  .command('prune')
  .description('Prune unreferenced packages from store')
  .action(async () => {
    const result = await mgpm.storePrune();
    console.log(result);
  });

program
  .command('store')
  .command('status')
  .description('Show store status')
  .action(async () => {
    const result = await mgpm.storeStatus();
    console.log(result);
  });

program
  .command('config')
  .command('get <key>')
  .description('Get a config value')
  .action(async (key) => {
    const result = await mgpm.configGet(key);
    console.log(result);
  });

program
  .command('config')
  .command('set <key> <value>')
  .description('Set a config value')
  .action(async (key, value) => {
    const result = await mgpm.configSet(key, value);
    console.log(result);
  });

program
  .command('config')
  .command('delete <key>')
  .description('Delete a config value')
  .action(async (key) => {
    const result = await mgpm.configDelete(key);
    console.log(result);
  });

program
  .command('config')
  .command('list')
  .description('List all configuration')
  .action(async () => {
    const result = await mgpm.configList();
    console.log(result);
  });

program
  .command('init')
  .description('Initialize a new project')
  .action(async () => {
    const result = await mgpm.init();
    console.log(result);
  });

program.parse(process.argv);
