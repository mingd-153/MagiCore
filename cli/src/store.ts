import { homedir } from 'os';
import { resolve, join, dirname } from 'path';
import { mkdir, access, constants, readFile, writeFile, stat } from 'fs/promises';
import { createHash } from 'crypto';
import { LockFile, LockPackage, StoreMeta } from './types.js';

export const DEFAULT_STORE_DIR = join(homedir(), '.megagate', 'store');
export const STORE_LAYOUT_VERSION = 'v1';

export function getStoreDir(customDir?: string): string {
    if (customDir) return resolve(customDir);
    const envDir = process.env.MEGAGATE_STORE_DIR;
    if (envDir) return resolve(envDir);
    return DEFAULT_STORE_DIR;
}

export function getStorePaths(storeDir: string) {
    const base = join(storeDir, STORE_LAYOUT_VERSION);
    return {
        base,
        files: join(base, 'files'),
        nodes: join(base, 'nodes'),
    };
}

export async function ensureStoreDirs(storeDir: string): Promise<void> {
    const { files, nodes } = getStorePaths(storeDir);
    await mkdir(files, { recursive: true });
    await mkdir(nodes, { recursive: true });
}

export function getPackageStorePath(storeDir: string, name: string, version: string): string {
    const { nodes } = getStorePaths(storeDir);
    const scopedName = name.startsWith('@') ? name.slice(1) : name;
    return join(nodes, scopedName, version);
}

export function getTarballPath(storeDir: string, name: string, version: string): string {
    const { files } = getStorePaths(storeDir);
    const safeName = name.replace('/', '+');
    return join(files, `${safeName}-${version}.tgz`);
}

export function getIntegrityPath(storeDir: string, name: string, version: string): string {
    return getTarballPath(storeDir, name, version) + '.sha512';
}

export function getMetaPath(storeDir: string, name: string, version: string): string {
    const pkgPath = getPackageStorePath(storeDir, name, version);
    return join(pkgPath, '.megagate-meta.json');
}

export async function writeStoreMeta(
    storeDir: string,
    name: string,
    version: string,
    meta: StoreMeta
): Promise<void> {
    const metaPath = getMetaPath(storeDir, name, version);
    await mkdir(dirname(metaPath), { recursive: true });
    await writeFile(metaPath, JSON.stringify(meta, null, 2), 'utf-8');
}

export async function readStoreMeta(
    storeDir: string,
    name: string,
    version: string
): Promise<StoreMeta | null> {
    const metaPath = getMetaPath(storeDir, name, version);
    try {
        const content = await readFile(metaPath, 'utf-8');
        return JSON.parse(content) as StoreMeta;
    } catch {
        return null;
    }
}

export function computeIntegrity(data: Buffer): string {
    const hash = createHash('sha512');
    hash.update(data);
    return `sha512-${hash.digest('base64')}`;
}

export async function verifyIntegrity(
    filePath: string,
    expectedIntegrity: string
): Promise<boolean> {
    const content = await readFile(filePath);
    const actual = computeIntegrity(content);
    return actual === expectedIntegrity;
}

export async function packageExistsInStore(
    storeDir: string,
    name: string,
    version: string
): Promise<boolean> {
    const pkgPath = getPackageStorePath(storeDir, name, version);
    try {
        await access(pkgPath, constants.R_OK);
        return true;
    } catch {
        return false;
    }
}

export async function getPackageSize(
    storeDir: string,
    name: string,
    version: string
): Promise<number> {
    const pkgPath = getPackageStorePath(storeDir, name, version);
    try {
        const stats = await stat(pkgPath);
        return stats.size;
    } catch {
        return 0;
    }
}

export function createVirtualStoreLink(projectNodeModules: string): string {
    return join(projectNodeModules, '.megagate');
}

export function createPackageLink(virtualStore: string, name: string, version: string): string {
    return join(virtualStore, `${name}@${version}`);
}

export function createNodeModulesLink(projectNodeModules: string, name: string): string {
    return join(projectNodeModules, name);
}
