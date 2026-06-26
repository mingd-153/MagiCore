import { readFile, writeFile, access, constants } from 'fs/promises';
import { resolve, dirname } from 'path';
import { LockFile, LockPackage, LockImporter, PackageJson } from './types.js';

const LOCK_VERSION = 1;
const LOCK_FILENAME = 'megagate-lock.json';

export function getLockPath(cwd: string = process.cwd()): string {
    return resolve(cwd, LOCK_FILENAME);
}

export async function loadLock(cwd: string = process.cwd()): Promise<LockFile | null> {
    const lockPath = getLockPath(cwd);
    try {
        await access(lockPath, constants.R_OK);
        const content = await readFile(lockPath, 'utf-8');
        const lock = JSON.parse(content) as LockFile;
        if (lock.version !== LOCK_VERSION) {
            throw new Error(`Unsupported lock file version: ${lock.version}`);
        }
        return lock;
    } catch (e: any) {
        if (e.code === 'ENOENT') return null;
        throw e;
    }
}

export async function saveLock(lock: LockFile, cwd: string = process.cwd()): Promise<void> {
    const lockPath = getLockPath(cwd);
    const content = JSON.stringify(lock, null, 2);
    await writeFile(lockPath, content, 'utf-8');
}

export function createEmptyLock(storeDir: string): LockFile {
    return {
        version: LOCK_VERSION,
        packages: {},
        importers: {
            '.': {
                dependencies: {},
                devDependencies: {},
                optionalDependencies: {},
            },
        },
        store: {
            dir: storeDir,
            layout: 'v1',
        },
    };
}

export function lockKey(name: string, version: string): string {
    return `${name}@${version}`;
}

export function parseLockKey(key: string): { name: string; version: string } {
    const atIndex = key.lastIndexOf('@');
    if (atIndex === -1) {
        throw new Error(`Invalid lock key: ${key}`);
    }
    return {
        name: key.slice(0, atIndex),
        version: key.slice(atIndex + 1),
    };
}

export function updateImporter(
    lock: LockFile,
    importerPath: string,
    deps: Record<string, string>,
    devDeps: Record<string, string>,
    optionalDeps: Record<string, string>
): void {
    lock.importers[importerPath] = {
        dependencies: deps,
        devDependencies: devDeps,
        optionalDependencies: optionalDeps,
    };
}

export function getImporterDeps(
    lock: LockFile,
    importerPath: string = '.'
): {
    deps: Record<string, string>;
    devDeps: Record<string, string>;
    optionalDeps: Record<string, string>;
} {
    const imp = lock.importers[importerPath];
    if (!imp) {
        return { deps: {}, devDeps: {}, optionalDeps: {} };
    }
    return {
        deps: imp.dependencies || {},
        devDeps: imp.devDependencies || {},
        optionalDeps: imp.optionalDependencies || {},
    };
}

export function addPackageToLock(lock: LockFile, pkg: LockPackage): void {
    const key = lockKey(pkg.name, pkg.version);
    lock.packages[key] = pkg;
}

export function packageExistsInLock(lock: LockFile, name: string, version: string): boolean {
    return lockKey(name, version) in lock.packages;
}

export function getPackageFromLock(
    lock: LockFile,
    name: string,
    version: string
): LockPackage | undefined {
    return lock.packages[lockKey(name, version)];
}

export function removePackageFromLock(lock: LockFile, name: string, version: string): boolean {
    const key = lockKey(name, version);
    if (key in lock.packages) {
        delete lock.packages[key];
        return true;
    }
    return false;
}

export function verifyLockIntegrity(lock: LockFile): { valid: boolean; errors: string[] } {
    const errors: string[] = [];
    for (const [key, pkg] of Object.entries(lock.packages)) {
        if (!pkg.integrity || !pkg.integrity.startsWith('sha512-')) {
            errors.push(`${key}: missing or invalid integrity`);
        }
        if (!pkg.resolved || !pkg.resolved.startsWith('http')) {
            errors.push(`${key}: missing resolved URL`);
        }
        if (pkg.size <= 0) {
            errors.push(`${key}: invalid size`);
        }
    }
    return { valid: errors.length === 0, errors };
}
