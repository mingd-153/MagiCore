// Minimum Release Age Checker
// Blocks packages published less than 24h ago (configurable)

import { LockedPackage } from '../types/index.js';
import { SecurityConfig, SecurityCheckResult } from './types.js';

export class MinimumReleaseAgeChecker {
    private minAgeHours: number;

    constructor(config: SecurityConfig) {
        this.minAgeHours = config.minimumReleaseAgeHours;
    }

    /**
     * Check if package passes minimum release age requirement
     * @returns { allowed, reason }
     */
    check(pkg: LockedPackage): SecurityCheckResult {
        if (!pkg.publishTime) {
            return {
                allowed: false,
                reason: `Package ${pkg.name}@${pkg.version} missing publishTime metadata`,
                blockedAt: new Date().toISOString(),
            };
        }

        const publishDate = new Date(pkg.publishTime);
        const now = new Date();
        const ageHours = (now.getTime() - publishDate.getTime()) / (1000 * 60 * 60);

        if (ageHours < this.minAgeHours) {
            console.warn(
                `[SECURITY] 🚫 BLOCKED by minimumReleaseAge (${this.minAgeHours}h): ` +
                    `${pkg.name}@${pkg.version} published ${ageHours.toFixed(2)}h ago ` +
                    `(${pkg.publishTime})`
            );
            return {
                allowed: false,
                reason: `Package published ${ageHours.toFixed(2)}h ago, minimum is ${this.minAgeHours}h`,
                blockedAt: now.toISOString(),
            };
        }

        console.log(
            `[SECURITY] ✅ PASSED minimumReleaseAge: ` +
                `${pkg.name}@${pkg.version} (${ageHours.toFixed(2)}h old, min ${this.minAgeHours}h)`
        );
        return { allowed: true };
    }

    /**
     * Check multiple packages
     */
    checkAll(packages: LockedPackage[]): { passed: LockedPackage[]; blocked: LockedPackage[] } {
        const passed: LockedPackage[] = [];
        const blocked: LockedPackage[] = [];

        for (const pkg of packages) {
            const result = this.check(pkg);
            if (result.allowed) {
                passed.push(pkg);
            } else {
                blocked.push(pkg);
            }
        }

        if (blocked.length > 0) {
            console.error(
                `[SECURITY] 🚫 ${blocked.length}/${packages.length} packages BLOCKED by minimumReleaseAge (${this.minAgeHours}h)`
            );
        } else {
            console.log(
                `[SECURITY] ✅ All ${packages.length} packages PASSED minimumReleaseAge (${this.minAgeHours}h)`
            );
        }

        return { passed, blocked };
    }
}

export function createMinimumReleaseAgeChecker(config: SecurityConfig): MinimumReleaseAgeChecker {
    return new MinimumReleaseAgeChecker(config);
}
