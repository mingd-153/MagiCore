import { MgpmCli, InstallOptions, AddOptions, UpdateOptions } from 'mgpm-native';

export class Mgpm {
  private cli: MgpmCli;

  constructor() {
    this.cli = new MgpmCli();
  }

  async install(opts?: Partial<InstallOptions>): Promise<string> {
    return this.cli.install({
      offline: false,
      dryRun: false,
      frozenLockfile: false,
      production: false,
      dev: false,
      optional: false,
      concurrency: 16,
      ...opts,
    });
  }

  async add(pkg: string, opts?: Partial<AddOptions>): Promise<string> {
    return this.cli.add(pkg, {
      dev: false,
      optional: false,
      peer: false,
      exact: false,
      save: true,
      ...opts,
    });
  }

  async remove(pkg: string): Promise<string> {
    return this.cli.remove(pkg);
  }

  async update(opts?: Partial<UpdateOptions>): Promise<string> {
    return this.cli.update({
      latest: false,
      save: true,
      dev: false,
      ...opts,
    });
  }

  async run(script: string, args?: string[]): Promise<string> {
    return this.cli.run(script, args ?? []);
  }

  async exec(command: string, args?: string[]): Promise<string> {
    return this.cli.exec(command, args ?? []);
  }

  async storePrune(): Promise<string> {
    return this.cli.storePrune();
  }

  async storeStatus(): Promise<string> {
    return this.cli.storeStatus();
  }

  async configGet(key: string): Promise<string> {
    return this.cli.configGet(key);
  }

  async configSet(key: string, value: string): Promise<string> {
    return this.cli.configSet(key, value);
  }

  async configDelete(key: string): Promise<string> {
    return this.cli.configDelete(key);
  }

  async configList(): Promise<string> {
    return this.cli.configList();
  }

  async init(): Promise<string> {
    return this.cli.init();
  }
}

export default Mgpm;
export { MgpmCli, InstallOptions, AddOptions, UpdateOptions } from 'mgpm-native';
