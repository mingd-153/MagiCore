// Connection Pool for HTTP requests
// Uses undici for high-performance HTTP/1.1 and HTTP/2

import { Agent, fetch, setGlobalDispatcher } from 'undici';
import { MegagateConfig } from '../types/index.js';

export interface PoolOptions {
  maxConcurrency: number;
  timeout: number;
  retries: number;
  keepAliveTimeout: number;
  keepAliveMaxTimeout: number;
}

export function createFetchPool(config: MegagateConfig): Agent {
  const pool = new Agent({
    connections: config.maxConcurrency,
    pipelining: 10,
    keepAliveTimeout: config.timeout || 30000,
    keepAliveMaxTimeout: config.timeout || 60000,
    connect: {
      timeout: config.timeout || 10000,
    },
    bodyTimeout: config.timeout || 30000,
    headersTimeout: config.timeout || 30000,
  });

  console.log(
    `[FETCHER] 🌐 Connection pool created: maxConcurrency=${config.maxConcurrency}, timeout=${config.timeout}ms`
  );

  return pool;
}

export function createFetchOptions(
  url: string,
  options: { headers?: Record<string, string>; signal?: AbortSignal } = {}
): RequestInit {
  return {
    method: 'GET',
    headers: {
      'Accept': 'application/vnd.npm.install-v1+json',
      'User-Agent': 'megagate-pm/0.1.0',
      ...options.headers,
    },
    signal: options.signal,
  };
}

export class FetchPool {
  private pool: Agent;
  private config: MegagateConfig;
  private previousDispatcher: any;

  constructor(config: MegagateConfig) {
    this.config = config;
    this.pool = createFetchPool(config);
    this.previousDispatcher = setGlobalDispatcher(this.pool);
  }

  async fetch(
    url: string,
    options: { headers?: Record<string, string>; retries?: number } = {}
  ): Promise<Response> {
    const retries = options.retries ?? 3;
    let lastError: Error | null = null;

    for (let attempt = 1; attempt <= retries; attempt++) {
      try {
        const controller = new AbortController();
        const timeoutId = setTimeout(() => controller.abort(), this.config.timeout);

        const response = await fetch(url, {
          ...createFetchOptions(url, { signal: controller.signal }),
          ...options,
          dispatcher: this.pool,
        } as any) as Response;

        clearTimeout(timeoutId);
        return response;
      } catch (error: any) {
        lastError = error;
        
        if (error.name === 'AbortError' || error.name === 'TimeoutError') {
          console.warn(
            `[FETCHER] ⏱️ Timeout fetching ${url} (attempt ${attempt}/${retries})`
          );
        } else {
          console.warn(
            `[FETCHER] ⚠️ Error fetching ${url} (attempt ${attempt}/${retries}): ${error.message}`
          );
        }

        if (attempt < retries) {
          const delay = Math.min(1000 * Math.pow(2, attempt - 1), 10000);
          console.log(`[FETCHER] 🔄 Retrying in ${delay}ms...`);
          await new Promise(r => setTimeout(r, delay));
        }
      }
    }

    throw lastError || new Error(`Failed to fetch ${url} after ${retries} attempts`);
  }

  async close(): Promise<void> {
    await this.pool.close();
    setGlobalDispatcher(this.previousDispatcher);
    console.log(`[FETCHER] 🔌 Connection pool closed`);
  }

  getStats() {
    return {
      maxConcurrency: this.config.maxConcurrency,
    };
  }
}

export function createFetchPoolInstance(config: MegagateConfig): FetchPool {
  return new FetchPool(config);
}
