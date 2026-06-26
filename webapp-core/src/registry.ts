import { RegistryPackage, RegistryVersion, ResolvedDependency } from './types.js';
import semver from 'semver';

const DEFAULT_REGISTRY = 'https://registry.npmjs.org';

export interface RegistryClientOptions {
    registry?: string;
    timeout?: number;
    retries?: number;
}

export class RegistryClient {
    private registry: string;
    private timeout: number;
    private retries: number;

    constructor(options: RegistryClientOptions = {}) {
        this.registry = options.registry || DEFAULT_REGISTRY;
        this.timeout = options.timeout || 30000;
        this.retries = options.retries || 3;
    }

    private async fetchWithRetry(url: string, attempt = 1): Promise<Response> {
        const controller = new AbortController();
        const timeoutId = setTimeout(() => controller.abort(), this.timeout);

        try {
            const response = await fetch(url, {
                signal: controller.signal,
                headers: {
                    Accept: 'application/json',
                    'User-Agent': 'megagate-pm/0.1.0',
                },
            });
            clearTimeout(timeoutId);
            return response;
        } catch (e: any) {
            clearTimeout(timeoutId);
            if (attempt < this.retries && (e.name === 'AbortError' || e.name === 'TimeoutError')) {
                await new Promise((r) => setTimeout(r, 1000 * attempt));
                return this.fetchWithRetry(url, attempt + 1);
            }
            throw e;
        }
    }

    async getPackageMetadata(name: string): Promise<RegistryPackage> {
        const encodedName = encodeURIComponent(name);
        const url = `${this.registry}/${encodedName}`;
        const response = await this.fetchWithRetry(url);

        if (!response.ok) {
            if (response.status === 404) {
                throw new Error(`Package not found: ${name}`);
            }
            throw new Error(`Registry error: ${response.status} ${response.statusText}`);
        }

        return response.json() as Promise<RegistryPackage>;
    }

    async getPackageVersion(name: string, version: string): Promise<RegistryPackage> {
        const encodedName = encodeURIComponent(name);
        const url = `${this.registry}/${encodedName}/${version}`;
        const response = await this.fetchWithRetry(url);

        if (!response.ok) {
            if (response.status === 404) {
                throw new Error(`Package version not found: ${name}@${version}`);
            }
            throw new Error(`Registry error: ${response.status} ${response.statusText}`);
        }

        return response.json() as Promise<RegistryPackage>;
    }

    async downloadTarball(url: string): Promise<Buffer> {
        const response = await this.fetchWithRetry(url);

        if (!response.ok) {
            throw new Error(
                `Failed to download tarball: ${response.status} ${response.statusText}`
            );
        }

        const arrayBuffer = await response.arrayBuffer();
        return Buffer.from(arrayBuffer);
    }

    private resolveLatestVersion(pkg: RegistryPackage, range: string): string | null {
        const versions = Object.keys(pkg.versions || {});
        return semver.maxSatisfying(versions, range, { includePrerelease: false }) || null;
    }

    async resolveDependency(name: string, range: string): Promise<ResolvedDependency> {
        const pkg = await this.getPackageMetadata(name);
        const version = this.resolveLatestVersion(pkg, range);

        if (!version) {
            throw new Error(`No matching version for ${name}@${range}`);
        }

        const versionData = pkg.versions?.[version] as RegistryVersion | undefined;
        if (!versionData) {
            throw new Error(`Version data missing for ${name}@${version}`);
        }

        return {
            name,
            version,
            integrity: versionData.dist.integrity,
            resolved: versionData.dist.tarball,
            size: versionData.dist.size,
            dependencies: versionData.dependencies,
            optionalDependencies: versionData.optionalDependencies,
            peerDependencies: versionData.peerDependencies,
            bin: versionData.bin,
            engines: versionData.engines,
        };
    }

    async resolveMultiple(
        deps: Record<string, string>,
        devDeps: Record<string, string> = {},
        optionalDeps: Record<string, string> = {}
    ): Promise<Map<string, ResolvedDependency>> {
        const allDeps = { ...deps, ...devDeps, ...optionalDeps };
        const results = new Map<string, ResolvedDependency>();

        for (const [name, range] of Object.entries(allDeps)) {
            const key = `${name}@${range}`;
            if (results.has(key)) continue;

            const resolved = await this.resolveDependency(name, range);
            results.set(key, resolved);
        }

        return results;
    }
}

export function createRegistryClient(options?: RegistryClientOptions): RegistryClient {
    return new RegistryClient(options);
}
