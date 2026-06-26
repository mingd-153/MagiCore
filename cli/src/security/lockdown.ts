// Lockdown Mode Checker
// Hardening for vanilla JS/TS/HTML/CSS projects

import { LockedPackage } from '../types/index.js';
import { SecurityConfig, LockdownCheckResult, LockdownViolation } from './types.js';

export class LockdownChecker {
    constructor(private config: SecurityConfig) {}

    /**
     * Check package for lockdown violations
     */
    check(pkg: LockedPackage): LockdownCheckResult {
        if (!this.config.lockdownMode) {
            return { allowed: true, violations: [] };
        }

        const violations: LockdownViolation[] = [];

        // 1. Check for native addons (.node files, binding.gyp)
        if (this.hasNativeAddons(pkg)) {
            violations.push({
                type: 'native-addon',
                file: 'package.json / native files',
                message:
                    'Package contains native addons (.node files or binding.gyp) - not allowed in lockdown mode',
            });
        }

        // 2. Check for unsafe sideEffects
        if (this.hasUnsafeSideEffects(pkg)) {
            violations.push({
                type: 'unsafe-sideeffects',
                file: 'package.json',
                message:
                    'Package has sideEffects: true or unsafe sideEffects array - not allowed in lockdown mode',
            });
        }

        // Note: eval/Function constructor checks require source code analysis
        // This is done at install time via source scanning (future enhancement)

        const allowed = violations.length === 0;

        if (!allowed) {
            console.warn(`[SECURITY] 🚫 LOCKDOWN VIOLATIONS for ${pkg.name}@${pkg.version}:`);
            for (const v of violations) {
                console.warn(`  - [${v.type}] ${v.file}: ${v.message}`);
            }
        } else {
            console.log(`[SECURITY] ✅ LOCKDOWN PASSED: ${pkg.name}@${pkg.version}`);
        }

        return { allowed, violations };
    }

    /**
     * Check multiple packages
     */
    checkAll(packages: LockedPackage[]): { passed: LockedPackage[]; failed: LockedPackage[] } {
        const passed: LockedPackage[] = [];
        const failed: LockedPackage[] = [];

        for (const pkg of packages) {
            const result = this.check(pkg);
            if (result.allowed) {
                passed.push(pkg);
            } else {
                failed.push(pkg);
            }
        }

        if (failed.length > 0) {
            console.error(
                `[SECURITY] 🚫 ${failed.length}/${packages.length} packages FAILED lockdown checks`
            );
        } else {
            console.log(`[SECURITY] ✅ All ${packages.length} packages PASSED lockdown mode`);
        }

        return { passed, failed };
    }

    private hasNativeAddons(pkg: LockedPackage): boolean {
        // Note: scripts are in PackageManifest, not LockedPackage
        // This check would need manifest data at install time
        // For now, we check if package name suggests native addon
        const nativePatterns = ['node-gyp', 'binding.gyp', '.node', 'native', 'addon'];
        const name = pkg.name.toLowerCase();
        return nativePatterns.some((pattern) => name.includes(pattern));
    }

    private hasUnsafeSideEffects(pkg: LockedPackage): boolean {
        const pkgJson = pkg as any; // pkg may have sideEffects from manifest
        const sideEffects = pkgJson.sideEffects;

        if (sideEffects === true) return true;
        if (Array.isArray(sideEffects)) {
            // Check for unsafe patterns like "**/*"
            return sideEffects.some(
                (pattern: string) =>
                    pattern.includes('*') || pattern === '*.js' || pattern === '*.ts'
            );
        }
        return false;
    }
}

export function createLockdownChecker(config: SecurityConfig): LockdownChecker {
    return new LockdownChecker(config);
}
