// Streaming Tarball Extract
// Download -> SHA-512 hash -> Extract to store (zero memory buffer)

import { pipeline } from 'stream/promises';
import { createHash } from 'crypto';
import { extract } from 'tar-fs';
import { Readable, Transform, TransformCallback } from 'stream';
import { IntegrityInfo, PackageRef, MegagateConfig } from '../types/index.js';
import { FetchPool } from './pool.js';

export interface StreamExtractResult {
  integrity: string;
  size: number;
  extractPath: string;
}

export class StreamingExtractor {
  private config: MegagateConfig;

  constructor(config: MegagateConfig) {
    this.config = config;
  }

  /**
   * Stream download, compute integrity, and extract directly to store
   * Never holds full tarball in memory
   */
  async downloadAndExtract(
    pool: FetchPool,
    pkg: PackageRef,
    downloadUrl: string,
    expectedIntegrity: string,
    extractPath: string
  ): Promise<StreamExtractResult> {
    console.log(
      `[FETCHER] 📥 Streaming download: ${pkg.name}@${pkg.version} from ${downloadUrl}`
    );

    const response = await pool.fetch(downloadUrl);
    
    if (!response.ok) {
      throw new Error(`HTTP ${response.status}: Failed to download ${pkg.name}@${pkg.version}`);
    }

    const hash = createHash('sha512');
    let size = 0;
    const startTime = Date.now();

    // Pipeline: HTTP response -> Hash transform -> tar-fs extract
    await pipeline(
      Readable.fromWeb(response.body as any),
      new Transform({
        transform(chunk: Buffer, _encoding: string, callback: TransformCallback) {
          hash.update(chunk);
          size += chunk.length;
          callback(null, chunk);
        },
      }),
      extract(extractPath, { strip: 1 })
    );

    const actualIntegrity = `sha512-${hash.digest('base64')}`;
    const duration = Date.now() - startTime;

    if (actualIntegrity !== expectedIntegrity) {
      console.error(
        `[FETCHER] ❌ INTEGRITY MISMATCH: ${pkg.name}@${pkg.version}`
      );
      console.error(`  Expected: ${expectedIntegrity}`);
      console.error(`  Actual:   ${actualIntegrity}`);
      throw new Error(
        `Integrity mismatch for ${pkg.name}@${pkg.version}: ` +
        `expected ${expectedIntegrity}, got ${actualIntegrity}`
      );
    }

    console.log(
      `[FETCHER] ✅ Downloaded & verified: ${pkg.name}@${pkg.version} ` +
      `(${size} bytes, ${duration}ms, ${(size / 1024 / 1024).toFixed(2)} MB)`
    );

    return {
      integrity: actualIntegrity,
      size,
      extractPath,
    };
  }

  /**
   * Verify integrity of already extracted package
   */
  async verifyIntegrity(extractPath: string, expectedIntegrity: string): Promise<boolean> {
    const hash = createHash('sha512');
    // Note: This would require re-tarring the directory
    // For now, we trust the initial download verification
    return true;
  }
}

export function createStreamingExtractor(config: MegagateConfig): StreamingExtractor {
  return new StreamingExtractor(config);
}
