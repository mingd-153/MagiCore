// Registry Client
// Fetches package metadata and tarballs from npm registry

import { FetchPool, createFetchPoolInstance } from './pool.js';
import { ResolvedDependency, MegagateConfig } from '../types/index.js';

export interface RegistryManifest {
  name: string;
  version: string;
  versions: Record<string, RegistryVersion>;
  'dist-tags': Record<string, string>;
  time: Record<string, string>;
}

export interface RegistryVersion {
  name: string;
  version: string;
  integrity: string;
  dependencies?: Record<string, string>;
  optionalDependencies?: Record<string, string>;
  peerDependencies?: Record<string, string>;
  bin?: Record<string, string>;
  engines?: Record<string, string>;
  dist: {
    tarball: string;
    shasum: string;
    integrity: string;
    size: number;
  };
  scripts?: Record<string, string>;
}

export class RegistryClient {
  private pool: FetchPool;
  private registryUrl: string;

  constructor(config: MegagateConfig) {
    this.registryUrl = config.registry.replace(/\/$/, '');
    this.pool = createFetchPoolInstance(config);
  }

  /**
   * Get full package metadata (all versions)
   */
  async getManifest(name: string): Promise<RegistryManifest> {
    const encodedName = encodeURIComponent(name);
    const url = `${this.registryUrl}/${encodedName}`;
    
    console.log(`[FETCHER] 📋 Fetching manifest: ${name}`);
    
    const response = await this.pool.fetch(url);
    
    if (response.status === 404) {
      throw new Error(`Package not found: ${name}`);
    }
    if (!response.ok) {
      throw new Error(`Registry error: ${response.status} ${response.statusText}`);
    }

    return response.json() as Promise<RegistryManifest>;
  }

  /**
   * Get specific version metadata
   */
  async getVersion(name: string, version: string): Promise<RegistryVersion> {
    const manifest = await this.getManifest(name);
    const versionData = manifest.versions[version];
    
    if (!versionData) {
      throw new Error(`Version ${version} not found for ${name}`);
    }

    return versionData;
  }

  /**
   * Resolve version range to concrete version
   */
  async resolveVersion(name: string, range: string): Promise<string> {
    const manifest = await this.getManifest(name);
    const versions = Object.keys(manifest.versions);
    
    // Simple semver resolution - in production use semver module
    const semver = await import('semver');
    const resolved = semver.maxSatisfying(versions, range, { includePrerelease: false });
    
    if (!resolved) {
      throw new Error(`No matching version for ${name}@${range}`);
    }
    
    console.log(`[FETCHER] 🎯 Resolved ${name}@${range} -> ${resolved}`);
    return resolved;
  }

  /**
   * Get package metadata with resolved version
   */
  async getPackageMetadata(name: string, range: string): Promise<{
    resolvedVersion: string;
    versionData: RegistryVersion;
  }> {
    const resolvedVersion = await this.resolveVersion(name, range);
    const versionData = await this.getVersion(name, resolvedVersion);
    
    return { resolvedVersion, versionData };
  }

  /**
   * Download tarball with streaming
   */
  async downloadTarball(url: string): Promise<Response> {
    const response = await this.pool.fetch(url);
    
    if (!response.ok) {
      throw new Error(`Failed to download tarball: ${response.status} ${response.statusText}`);
    }
    
    return response;
  }

  /**
   * Close connection pool
   */
  async close(): Promise<void> {
    await this.pool.close();
  }
}

export function createRegistryClient(config: MegagateConfig): RegistryClient {
  return new RegistryClient(config);
}
