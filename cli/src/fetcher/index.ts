// Fetcher - Main Entry
// Orchestrates: Registry -> Streaming Download -> Store

import { FetchPool, createFetchPoolInstance } from './pool.js';
import { StreamingExtractor, createStreamingExtractor } from './streamExtract.js';
import { RegistryClient, createRegistryClient, RegistryVersion } from './registry.js';
import {
    StoreBackend,
    PackageRef,
    IntegrityInfo,
    PackageManifest,
    ResolvedDependency,
    MegagateConfig,
} from '../types/index.js';
import { Readable } from 'stream';

export interface FetchResult {
    tarballPath: string;
    extractPath: string;
    integrity: string;
    size: number;
}

export class Fetcher {
    private pool: FetchPool;
    private extractor: StreamingExtractor;
    private registry: RegistryClient;
    private store: StoreBackend;
    private config: MegagateConfig;
    private fetchCache = new Map<string, FetchResult>();

    constructor(store: StoreBackend, config: MegagateConfig) {
        this.store = store;
        this.config = config;
        this.pool = createFetchPoolInstance(config);
        this.extractor = createStreamingExtractor(config);
        this.registry = createRegistryClient(config);
    }

    /**
     * Fetch and extract a single package
     */
    async fetchSingle(pkg: ResolvedDependency): Promise<FetchResult> {
        const cacheKey = `${pkg.name}@${pkg.version}`;

        // Check cache
        if (this.fetchCache.has(cacheKey)) {
            console.log(`[FETCHER] 💾 Cache hit: ${cacheKey}`);
            return this.fetchCache.get(cacheKey)!;
        }

        // Check if already in store
        const pkgRef: PackageRef = { name: pkg.name, version: pkg.version };
        if (await this.store.exists(pkgRef)) {
            console.log(`[FETCHER] 📦 Already in store: ${cacheKey}`);
            const result: FetchResult = {
                tarballPath: '',
                extractPath: this.store.getPath(pkgRef),
                integrity: pkg.integrity,
                size: pkg.size,
            };
            this.fetchCache.set(cacheKey, result);
            return result;
        }

        // Download and extract
        const extractPath = this.store.getPath(pkgRef);

        try {
            const result = await this.extractor.downloadAndExtract(
                this.pool,
                pkgRef,
                pkg.resolved,
                pkg.integrity,
                extractPath
            );

            // Write manifest and metadata to store
            const manifest: PackageManifest = {
                name: pkg.name,
                version: pkg.version,
                dependencies: pkg.dependencies,
                optionalDependencies: pkg.optionalDependencies,
                peerDependencies: pkg.peerDependencies,
                bin: pkg.bin,
                engines: pkg.engines,
                scripts: pkg.scripts,
            };

            await this.store.writeManifest(pkgRef, manifest);
            await this.store.writeMetadata(pkgRef, {
                integrity: result.integrity,
                size: result.size,
                extractedAt: new Date().toISOString(),
                publishTime: pkg.publishTime,
                approvedBuilds: pkg.approvedBuilds,
            });

            const fetchResult: FetchResult = {
                tarballPath: '',
                extractPath,
                integrity: result.integrity,
                size: result.size,
            };

            this.fetchCache.set(cacheKey, fetchResult);
            return fetchResult;
        } catch (error) {
            // Clean up on failure
            try {
                await this.store.remove(pkgRef);
            } catch {
                // ignore cleanup errors
            }
            throw error;
        }
    }

    /**
     * Fetch multiple packages with concurrency control
     */
    async fetchMultiple(packages: ResolvedDependency[]): Promise<Map<string, FetchResult>> {
        const results = new Map<string, FetchResult>();
        const concurrency = this.config.maxConcurrency;
        const queue = [...packages];
        const running = new Set<Promise<void>>();

        console.log(
            `[FETCHER] 🚀 Starting fetch of ${packages.length} packages (concurrency: ${concurrency})`
        );

        const processNext = async (): Promise<void> => {
            if (queue.length === 0) return;

            const pkg = queue.shift()!;
            const key = `${pkg.name}@${pkg.version}`;

            const promise = this.fetchSingle(pkg)
                .then((result: FetchResult) => {
                    results.set(key, result);
                })
                .finally(() => {
                    running.delete(promise);
                    return processNext();
                });

            running.add(promise);

            if (running.size >= concurrency) {
                await Promise.race(running);
            }

            return processNext();
        };

        await Promise.all(
            Array.from({ length: Math.min(concurrency, packages.length) }, () => processNext())
        );

        console.log(`[FETCHER] ✅ Completed: ${results.size}/${packages.length} packages fetched`);

        return results;
    }

    /**
     * Fetch package metadata from registry
     */
    async getPackageMetadata(name: string, range: string) {
        return this.registry.getPackageMetadata(name, range);
    }

    /**
     * Close connections
     */
    async close(): Promise<void> {
        await this.pool.close();
    }
}

export function createFetcher(store: StoreBackend, config: MegagateConfig): Fetcher {
    return new Fetcher(store, config);
}
