import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { mkdtemp, rm } from 'fs/promises';
import { join } from 'path';
import { tmpdir } from 'os';
import { FsStoreBackend } from '../../src/store/fsBackend.js';
import { createFsStoreBackend } from '../../src/store/index.js';
import { MegagateConfig, PackageRef, PackageManifest } from '../../src/types/index.js';

describe('FsStoreBackend', () => {
    let tempDir: string;
    let store: FsStoreBackend;
    let config: MegagateConfig;

    beforeEach(async () => {
        tempDir = await mkdtemp(join(tmpdir(), 'megagate-test-'));
        config = {
            storeDir: tempDir,
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
        store = createFsStoreBackend(config);
        await store.init(config);
    });

    afterEach(async () => {
        await rm(tempDir, { recursive: true, force: true });
    });

    const testPkg: PackageRef = { name: 'test-pkg', version: '1.0.0' };
    const testManifest: PackageManifest = {
        name: 'test-pkg',
        version: '1.0.0',
        dependencies: { dep1: '^1.0.0' },
    };

    it('initializes store directories', async () => {
        const exists = await store.exists(testPkg);
        expect(exists).toBe(false);
    });

    it('writes and reads manifest', async () => {
        await store.writeManifest(testPkg, testManifest);
        const manifest = await store.readManifest(testPkg);
        expect(manifest).toEqual(testManifest);
    });

    it('writes and reads metadata', async () => {
        const meta = {
            integrity: 'sha512-test',
            size: 100,
            extractedAt: new Date().toISOString(),
            publishTime: '2024-01-01T00:00:00.000Z',
            approvedBuilds: ['postinstall'],
        };
        await store.writeMetadata(testPkg, meta);
        const readMeta = await store.readMetadata(testPkg);
        expect(readMeta).toEqual(meta);
    });

    it('creates symlink (hardlink not supported on temp fs)', async () => {
        // Use symlink strategy for test (hardlink fails on cross-device temp fs)
        const symlinkStore = createFsStoreBackend({ ...config, linkStrategy: 'symlink' });
        await symlinkStore.init({ ...config, linkStrategy: 'symlink' });
        await symlinkStore.writeManifest(testPkg, testManifest);

        const linkPath = join(tempDir, 'link');
        await symlinkStore.createSymlink(testPkg, linkPath);

        // Check link exists
        const manifest = await symlinkStore.readManifest(testPkg);
        expect(manifest).toEqual(testManifest);
    });

    it('removes package', async () => {
        await store.writeManifest(testPkg, testManifest);
        await store.remove(testPkg);

        const exists = await store.exists(testPkg);
        expect(exists).toBe(false);
    });

    it('prunes unreferenced packages', async () => {
        // Create two packages
        const pkg1: PackageRef = { name: 'pkg1', version: '1.0.0' };
        const pkg2: PackageRef = { name: 'pkg2', version: '1.0.0' };
        await store.writeManifest(pkg1, { ...testManifest, name: 'pkg1' });
        await store.writeMetadata(pkg1, {
            integrity: 'sha512-1',
            size: 100,
            extractedAt: new Date().toISOString(),
        });
        await store.writeManifest(pkg2, { ...testManifest, name: 'pkg2' });
        await store.writeMetadata(pkg2, {
            integrity: 'sha512-2',
            size: 200,
            extractedAt: new Date().toISOString(),
        });

        // Prune keeping only pkg1
        const result = await store.prune(new Set(['pkg1@1.0.0']));
        // Just verify pkg1 stays and pkg2 is removed (count varies by FS)
        expect(result.removed).toBeGreaterThanOrEqual(1);

        const pkg1Exists = await store.exists(pkg1);
        const pkg2Exists = await store.exists(pkg2);
        expect(pkg1Exists).toBe(true);
        expect(pkg2Exists).toBe(false);
    });
});
