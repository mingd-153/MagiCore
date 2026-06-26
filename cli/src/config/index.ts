// MegaGate Config System - TOML parser + loader
// Zero external dependencies

import { readFile } from 'fs/promises';
import { resolve, join } from 'path';
import { homedir } from 'os';
import { MegagateConfig, WorkspaceConfig } from '../types/index.js';

const DEFAULT_CONFIG: MegagateConfig = {
    storeDir: join(homedir(), '.megagate', 'store'),
    registry: 'https://registry.npmjs.org',
    minimumReleaseAgeHours: 24,
    approveBuilds: true,
    lockdownMode: false,
    linkStrategy: 'hardlink',
    maxConcurrency: 16,
    offlineMode: false,
    preferOffline: false,
    timeout: 30000,
    retries: 3,
};

export interface ConfigFile {
    security?: {
        minimumReleaseAgeHours?: number;
        approveBuilds?: boolean;
        lockdownMode?: boolean;
    };
    store?: {
        dir?: string;
        linkStrategy?: 'hardlink' | 'symlink' | 'copy';
    };
    network?: {
        registry?: string;
        maxConcurrency?: number;
        timeout?: number;
        retries?: number;
    };
    build?: {
        target?: string;
        format?: string;
        sourcemap?: boolean;
    };
    workspace?: WorkspaceConfig;
}

export function parseTOML(content: string): ConfigFile {
    const result: ConfigFile = {};
    let currentSection = '';

    for (const line of content.split('\n')) {
        const trimmed = line.trim();
        if (!trimmed || trimmed.startsWith('#')) continue;

        const sectionMatch = trimmed.match(/^\[(.+)\]$/);
        if (sectionMatch) {
            currentSection = sectionMatch[1];
            continue;
        }

        const kvMatch = trimmed.match(/^(\w+)\s*=\s*(.+)$/);
        if (kvMatch && currentSection) {
            const key = kvMatch[1];
            let value: any = kvMatch[2].trim();

            if (value.startsWith('"') && value.endsWith('"')) {
                value = value.slice(1, -1);
            } else if (value === 'true') {
                value = true;
            } else if (value === 'false') {
                value = false;
            } else if (/^\d+$/.test(value)) {
                value = parseInt(value, 10);
            }

            const sections = currentSection.split('.');
            let obj: any = result;
            for (const s of sections) {
                obj[s] = obj[s] || {};
                obj = obj[s];
            }
            obj[key] = value;
        }
    }

    return result;
}

function mergeConfig(base: MegagateConfig, file: ConfigFile): MegagateConfig {
    const config = { ...base };

    if (file.security) {
        if (file.security.minimumReleaseAgeHours !== undefined) {
            config.minimumReleaseAgeHours = file.security.minimumReleaseAgeHours;
        }
        if (file.security.approveBuilds !== undefined) {
            config.approveBuilds = file.security.approveBuilds;
        }
        if (file.security.lockdownMode !== undefined) {
            config.lockdownMode = file.security.lockdownMode;
        }
    }

    if (file.store) {
        if (file.store.dir) config.storeDir = file.store.dir;
        if (file.store.linkStrategy) config.linkStrategy = file.store.linkStrategy;
    }

    if (file.network) {
        if (file.network.registry) config.registry = file.network.registry;
        if (file.network.maxConcurrency) config.maxConcurrency = file.network.maxConcurrency;
    }

    if (file.build) {
        // Build config stored separately
    }

    return config;
}

function applyEnvOverrides(config: MegagateConfig): MegagateConfig {
    const result = { ...config };

    if (process.env.MEGAGATE_STORE_DIR) {
        result.storeDir = process.env.MEGAGATE_STORE_DIR;
    }
    if (process.env.MEGAGATE_REGISTRY) {
        result.registry = process.env.MEGAGATE_REGISTRY;
    }
    if (process.env.MEGAGATE_MINIMUM_RELEASE_AGE) {
        result.minimumReleaseAgeHours = parseInt(process.env.MEGAGATE_MINIMUM_RELEASE_AGE, 10);
    }
    if (process.env.MEGAGATE_APPROVE_BUILDS) {
        result.approveBuilds = process.env.MEGAGATE_APPROVE_BUILDS === 'true';
    }
    if (process.env.MEGAGATE_LOCKDOWN_MODE) {
        result.lockdownMode = process.env.MEGAGATE_LOCKDOWN_MODE === 'true';
    }
    if (process.env.MEGAGATE_MAX_CONCURRENCY) {
        result.maxConcurrency = parseInt(process.env.MEGAGATE_MAX_CONCURRENCY, 10);
    }
    if (process.env.MEGAGATE_OFFLINE) {
        result.offlineMode = process.env.MEGAGATE_OFFLINE === 'true';
    }
    if (process.env.MEGAGATE_PREFER_OFFLINE) {
        result.preferOffline = process.env.MEGAGATE_PREFER_OFFLINE === 'true';
    }

    return result;
}

export async function loadConfig(cwd: string = process.cwd()): Promise<{
    config: MegagateConfig;
    workspaceConfig: WorkspaceConfig | null;
}> {
    let config = DEFAULT_CONFIG;
    let workspaceConfig: WorkspaceConfig | null = null;

    // Load project config
    const projectConfigPath = resolve(cwd, 'megagate.toml');
    try {
        const content = await readFile(projectConfigPath, 'utf-8');
        const parsed = parseTOML(content);
        config = mergeConfig(config, parsed);
        workspaceConfig = parsed.workspace || null;
    } catch {
        // No project config, use defaults
    }

    // Load global config
    const globalConfigPath = join(homedir(), '.megagaterc');
    try {
        const content = await readFile(globalConfigPath, 'utf-8');
        const parsed = parseTOML(content);
        config = mergeConfig(config, parsed);
    } catch {
        // No global config
    }

    // Apply environment overrides
    config = applyEnvOverrides(config);

    return { config, workspaceConfig };
}

export function getDefaultConfig(): MegagateConfig {
    return { ...DEFAULT_CONFIG };
}

export function validateConfig(config: MegagateConfig): { valid: boolean; errors: string[] } {
    const errors: string[] = [];

    if (config.minimumReleaseAgeHours < 0) {
        errors.push('minimumReleaseAgeHours must be >= 0');
    }
    if (config.maxConcurrency < 1) {
        errors.push('maxConcurrency must be >= 1');
    }
    if (!['hardlink', 'symlink', 'copy'].includes(config.linkStrategy)) {
        errors.push('linkStrategy must be hardlink, symlink, or copy');
    }
    if (!config.registry.startsWith('http')) {
        errors.push('registry must be a valid URL');
    }

    return { valid: errors.length === 0, errors };
}
