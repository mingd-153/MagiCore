import { describe, it, expect } from 'vitest';
import { loadConfig, parseTOML, validateConfig, getDefaultConfig } from '../../src/config/index.js';
import { join } from 'path';

describe('Config System', () => {
    describe('parseTOML', () => {
        it('parses basic config', () => {
            const toml = `
[security]
minimumReleaseAgeHours = 48
approveBuilds = false

[store]
dir = "/custom/store"
linkStrategy = "symlink"
`;
            const parsed = parseTOML(toml);
            expect(parsed.security?.minimumReleaseAgeHours).toBe(48);
            expect(parsed.security?.approveBuilds).toBe(false);
            expect(parsed.store?.dir).toBe('/custom/store');
            expect(parsed.store?.linkStrategy).toBe('symlink');
        });

        it('handles comments and empty lines', () => {
            const toml = `
# This is a comment
[security]
minimumReleaseAgeHours = 12

# Another comment
[store]
dir = "/store"
`;
            const parsed = parseTOML(toml);
            expect(parsed.security?.minimumReleaseAgeHours).toBe(12);
        });
    });

    describe('validateConfig', () => {
        it('validates default config', () => {
            const config = getDefaultConfig();
            const result = validateConfig(config);
            expect(result.valid).toBe(true);
        });

        it('rejects negative minimumReleaseAgeHours', () => {
            const config = getDefaultConfig();
            config.minimumReleaseAgeHours = -1;
            const result = validateConfig(config);
            expect(result.valid).toBe(false);
            expect(result.errors).toContain('minimumReleaseAgeHours must be >= 0');
        });

        it('rejects invalid linkStrategy', () => {
            const config = getDefaultConfig();
            config.linkStrategy = 'invalid' as any;
            const result = validateConfig(config);
            expect(result.valid).toBe(false);
            expect(result.errors).toContain('linkStrategy must be hardlink, symlink, or copy');
        });
    });

    describe('getDefaultConfig', () => {
        it('returns expected defaults', () => {
            const config = getDefaultConfig();
            expect(config.minimumReleaseAgeHours).toBe(24);
            expect(config.approveBuilds).toBe(true);
            expect(config.lockdownMode).toBe(false);
            expect(config.linkStrategy).toBe('hardlink');
            expect(config.maxConcurrency).toBe(16);
        });
    });
});
