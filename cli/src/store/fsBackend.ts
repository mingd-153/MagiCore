// MegaGate FS Store Backend - Content-addressable store with hardlinks
// Implements pnpm-style store layout

import {
    mkdir,
    writeFile,
    readFile,
    access,
    constants,
    stat,
    unlink,
    rm,
    readdir,
    realpath,
    symlink,
    link,
} from 'fs/promises';
import { join, dirname, resolve, relative } from 'path';
import { createHash } from 'crypto';
import { pipeline } from 'stream/promises';
import { extract } from 'tar-fs';
import { Readable, Transform, TransformCallback } from 'stream';
import {
    StoreBackend,
    PackageRef,
    IntegrityInfo,
    PackageManifest,
    PackageMetadata,
    PruneResult,
    MegagateConfig,
} from './index.js';

const STORE_LAYOUT_VERSION = 'v1';

export class FsStoreBackend implements StoreBackend {
    private rootDir: string;
    private filesDir: string;
    private nodesDir: string;
    private linkStrategy: 'hardlink' | 'symlink' | 'copy';

    constructor(config: MegagateConfig) {
        this.rootDir = resolve(config.storeDir);
        this.filesDir = join(this.rootDir, STORE_LAYOUT_VERSION, 'files');
        this.nodesDir = join(this.rootDir, STORE_LAYOUT_VERSION, 'nodes');
        this.linkStrategy = config.linkStrategy;
    }

    async init(config: MegagateConfig): Promise<void> {
        await mkdir(this.filesDir, { recursive: true });
        await mkdir(this.nodesDir, { recursive: true });
    }

    exists(pkg: PackageRef): Promise<boolean> {
        const pkgPath = this.getPackageNodePath(pkg);
        return access(pkgPath, constants.R_OK)
            .then(() => true)
            .catch(() => false);
    }

    getPath(pkg: PackageRef): string {
        return this.getPackageNodePath(pkg);
    }

    private getPackageNodePath(pkg: PackageRef): string {
        const scopedName = pkg.name.startsWith('@') ? pkg.name.slice(1) : pkg.name;
        return join(this.nodesDir, scopedName, pkg.version);
    }

    private getTarballPath(pkg: PackageRef): string {
        const safeName = pkg.name.replace('/', '+');
        return join(this.filesDir, `${safeName}-${pkg.version}.tgz`);
    }

    private getIntegrityPath(pkg: PackageRef): string {
        return this.getTarballPath(pkg) + '.sha512';
    }

    private getManifestPath(pkg: PackageRef): string {
        return join(this.getPackageNodePath(pkg), 'package.json');
    }

    private getMetadataPath(pkg: PackageRef): string {
        return join(this.getPackageNodePath(pkg), '.megagate-meta.json');
    }

    private getNodeModulesPath(pkg: PackageRef): string {
        return join(this.getPackageNodePath(pkg), 'node_modules');
    }

    async writeTarball(pkg: PackageRef, stream: Readable): Promise<IntegrityInfo> {
        const tarballPath = this.getTarballPath(pkg);
        const integrityPath = this.getIntegrityPath(pkg);
        const extractPath = this.getPackageNodePath(pkg);

        await mkdir(dirname(tarballPath), { recursive: true });
        await mkdir(extractPath, { recursive: true });

        const hash = createHash('sha512');
        let size = 0;

        // Stream: download -> hash -> save tarball -> extract
        await pipeline(
            stream,
            new Transform({
                transform(chunk: Buffer, _encoding: string, callback: TransformCallback) {
                    hash.update(chunk);
                    size += chunk.length;
                    callback(null, chunk);
                },
            }),
            extract(extractPath, { strip: 1 })
        );

        const integrity = `sha512-${hash.digest('base64')}`;

        await writeFile(tarballPath, ''); // Touch file (content already extracted)
        await writeFile(integrityPath, integrity, 'utf-8');

        return { integrity, size };
    }

    async readTarball(pkg: PackageRef): Promise<Readable> {
        const tarballPath = this.getTarballPath(pkg);
        const { createReadStream } = await import('fs');
        return createReadStream(tarballPath);
    }

    async writeManifest(pkg: PackageRef, manifest: PackageManifest): Promise<void> {
        const manifestPath = this.getManifestPath(pkg);
        await mkdir(dirname(manifestPath), { recursive: true });
        await writeFile(manifestPath, JSON.stringify(manifest, null, 2), 'utf-8');
    }

    async readManifest(pkg: PackageRef): Promise<PackageManifest | null> {
        try {
            const content = await readFile(this.getManifestPath(pkg), 'utf-8');
            return JSON.parse(content) as PackageManifest;
        } catch {
            return null;
        }
    }

    async writeMetadata(pkg: PackageRef, meta: PackageMetadata): Promise<void> {
        const metaPath = this.getMetadataPath(pkg);
        await mkdir(dirname(metaPath), { recursive: true });
        await writeFile(metaPath, JSON.stringify(meta, null, 2), 'utf-8');
    }

    async readMetadata(pkg: PackageRef): Promise<PackageMetadata | null> {
        try {
            const content = await readFile(this.getMetadataPath(pkg), 'utf-8');
            return JSON.parse(content) as PackageMetadata;
        } catch {
            return null;
        }
    }

    private async ensureLinkStrategy(target: string, linkPath: string): Promise<void> {
        await mkdir(dirname(linkPath), { recursive: true });

        // Remove existing
        try {
            await unlink(linkPath);
        } catch {
            // ignore
        }

        switch (this.linkStrategy) {
            case 'hardlink':
                await link(target, linkPath);
                break;
            case 'symlink':
                await symlink(target, linkPath, 'dir');
                break;
            case 'copy':
                await this.copyDir(target, linkPath);
                break;
        }
    }

    private async copyDir(src: string, dest: string): Promise<void> {
        await mkdir(dest, { recursive: true });
        const entries = await readdir(src, { withFileTypes: true });
        for (const entry of entries) {
            const srcPath = join(src, entry.name);
            const destPath = join(dest, entry.name);
            if (entry.isDirectory()) {
                await this.copyDir(srcPath, destPath);
            } else {
                await this.copyFile(srcPath, destPath);
            }
        }
    }

    private async copyFile(src: string, dest: string): Promise<void> {
        const { copyFile } = await import('fs/promises');
        await copyFile(src, dest);
    }

    async createHardlink(pkg: PackageRef, target: string): Promise<void> {
        await this.ensureLinkStrategy(this.getPackageNodePath(pkg), target);
    }

    async createSymlink(pkg: PackageRef, target: string): Promise<void> {
        await this.ensureLinkStrategy(this.getPackageNodePath(pkg), target);
    }

    async remove(pkg: PackageRef): Promise<void> {
        const pkgPath = this.getPackageNodePath(pkg);
        await rm(pkgPath, { recursive: true, force: true });

        // Also remove tarball and integrity
        try {
            await unlink(this.getTarballPath(pkg));
        } catch {
            // ignore
        }
        try {
            await unlink(this.getIntegrityPath(pkg));
        } catch {
            // ignore
        }
    }

    async prune(referenced: Set<string>): Promise<PruneResult> {
        let removed = 0;
        let freedBytes = 0;

        const entries = await readdir(this.nodesDir, { withFileTypes: true }).catch(() => []);

        for (const entry of entries) {
            if (!entry.isDirectory()) continue;
            
            const entryPath = join(this.nodesDir, entry.name);
            
            // Check if this is a scoped package (@scope/name) or regular package (name)
            if (entry.name.startsWith('@')) {
                // Scoped package: @scope/name/version
                const scopePath = entryPath;
                const packages = await readdir(scopePath, { withFileTypes: true }).catch(() => []);
                
                for (const pkgEntry of packages) {
                    if (!pkgEntry.isDirectory()) continue;
                    
                    const pkgPath = join(scopePath, pkgEntry.name);
                    const versions = await readdir(pkgPath, { withFileTypes: true }).catch(() => []);
                    
                    for (const versionEntry of versions) {
                        if (!versionEntry.isDirectory()) continue;
                        
                        const versionPath = join(pkgPath, versionEntry.name);
                        const key = `${entry.name}/${pkgEntry.name}@${versionEntry.name}`;
                        
                        if (!referenced.has(key)) {
                            const stats = await stat(versionPath).catch(() => null);
                            if (stats) {
                                freedBytes += stats.size;
                            }
                            await rm(versionPath, { recursive: true, force: true });
                            removed++;
                        }
                    }
                    
                    // Clean empty package dir
                    const remaining = await readdir(pkgPath).catch(() => []);
                    if (remaining.length === 0) {
                        await rm(pkgPath, { recursive: true, force: true });
                    }
                }
                
                // Clean empty scope dir
                const scopeRemaining = await readdir(scopePath).catch(() => []);
                if (scopeRemaining.length === 0) {
                    await rm(scopePath, { recursive: true, force: true });
                }
            } else {
                // Regular package: name/version
                const pkgPath = entryPath;
                const versions = await readdir(pkgPath, { withFileTypes: true }).catch(() => []);
                
                for (const versionEntry of versions) {
                    if (!versionEntry.isDirectory()) continue;
                    
                    const versionPath = join(pkgPath, versionEntry.name);
                    const key = `${entry.name}@${versionEntry.name}`;
                    
                    if (!referenced.has(key)) {
                        const stats = await stat(versionPath).catch(() => null);
                        if (stats) {
                            freedBytes += stats.size;
                        }
                        await rm(versionPath, { recursive: true, force: true });
                        removed++;
                    }
                }
                
                // Clean empty package dir
                const remaining = await readdir(pkgPath).catch(() => []);
                if (remaining.length === 0) {
                    await rm(pkgPath, { recursive: true, force: true });
                }
            }
        }

        return { removed, freedBytes };
    }

    async verifyIntegrity(pkg: PackageRef): Promise<boolean> {
        const integrityPath = this.getIntegrityPath(pkg);
        const tarballPath = this.getTarballPath(pkg);

        try {
            const expected = await readFile(integrityPath, 'utf-8');
            const hash = createHash('sha512');
            const { createReadStream } = await import('fs');

            await pipeline(
                createReadStream(tarballPath),
                new Transform({
                    transform(chunk: Buffer, _encoding: string, callback: TransformCallback) {
                        hash.update(chunk);
                        callback(null, chunk);
                    },
                })
            );

            const actual = `sha512-${hash.digest('base64')}`;
            return actual === expected.trim();
        } catch {
            return false;
        }
    }
}

export function createFsStoreBackend(config: MegagateConfig): FsStoreBackend {
    return new FsStoreBackend(config);
}
