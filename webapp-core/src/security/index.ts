// Security Module - Main Entry
// Combines: minimumReleaseAge, approveBuilds, lockdown

import { LockedPackage } from '../types/index.js';
import {
    SecurityConfig,
    SecurityCheckResult,
    LockdownCheckResult,
    LockdownViolation,
    ApprovedBuildsRecord,
    LifecycleScriptCheck,
} from './types.js';
import { MinimumReleaseAgeChecker, createMinimumReleaseAgeChecker } from './minimumReleaseAge.js';
import { ApproveBuildsManager, createApproveBuildsManager } from './approveBuilds.js';
import { LockdownChecker, createLockdownChecker } from './lockdown.js';

export class SecurityManager {
    private minimumReleaseAge: MinimumReleaseAgeChecker;
    private approveBuilds: ApproveBuildsManager;
    private lockdown: LockdownChecker;

    constructor(config: SecurityConfig) {
        this.minimumReleaseAge = createMinimumReleaseAgeChecker(config);
        this.approveBuilds = createApproveBuildsManager(config);
        this.lockdown = createLockdownChecker(config);
    }

    /**
     * Load approved builds from lockfile
     */
    loadApprovedBuilds(approvedBuilds: Record<string, string[]>): void {
        this.approveBuilds.loadFromLockfile(approvedBuilds);
    }

    /**
     * Get current approved builds for saving to lockfile
     */
    getApprovedBuilds(): Record<string, string[]> {
        return this.approveBuilds.getApprovedBuilds();
    }

    /**
     * Check package against all security policies
     * @returns { allowed, results }
     */
    checkPackage(pkg: LockedPackage): {
        allowed: boolean;
        minimumReleaseAge: SecurityCheckResult;
        lockdown: LockdownCheckResult;
        unapprovedScripts: string[];
    } {
        const minimumReleaseAge = this.minimumReleaseAge.check(pkg);
        const lockdown = this.lockdown.check(pkg);
        const unapprovedScripts = this.approveBuilds
            .checkAll(pkg)
            .filter((c) => !c.approved)
            .map((c) => c.script);

        const allowed = minimumReleaseAge.allowed && lockdown.allowed;

        return { allowed, minimumReleaseAge, lockdown, unapprovedScripts };
    }

    /**
     * Check multiple packages
     */
    checkAllPackages(packages: LockedPackage[]): {
        allowed: LockedPackage[];
        blocked: LockedPackage[];
        summary: {
            total: number;
            passed: number;
            blockedByAge: number;
            blockedByLockdown: number;
            withUnapprovedScripts: number;
        };
    } {
        const allowed: LockedPackage[] = [];
        const blocked: LockedPackage[] = [];
        let blockedByAge = 0;
        let blockedByLockdown = 0;
        let withUnapprovedScripts = 0;

        console.log(
            `[SECURITY] 🔍 Checking ${packages.length} packages against security policies...`
        );

        for (const pkg of packages) {
            const result = this.checkPackage(pkg);

            if (result.unapprovedScripts.length > 0) {
                withUnapprovedScripts++;
            }

            if (!result.minimumReleaseAge.allowed) {
                blockedByAge++;
            }
            if (!result.lockdown.allowed) {
                blockedByLockdown++;
            }

            if (result.allowed) {
                allowed.push(pkg);
            } else {
                blocked.push(pkg);
            }
        }

        const summary = {
            total: packages.length,
            passed: allowed.length,
            blockedByAge,
            blockedByLockdown,
            withUnapprovedScripts,
        };

        console.log(
            `[SECURITY] 📊 SUMMARY: ${summary.passed}/${summary.total} passed, ` +
                `${blockedByAge} blocked by age, ${blockedByLockdown} by lockdown, ` +
                `${withUnapprovedScripts} have unapproved scripts`
        );

        return { allowed, blocked, summary };
    }

    /**
     * Approve a lifecycle script for a package
     */
    approveScript(
        pkg: LockedPackage,
        script: 'prepare' | 'preinstall' | 'postinstall' | 'prepublish' | 'prepack'
    ): void {
        this.approveBuilds.approve(pkg, script);
    }

    /**
     * Check if a script is approved
     */
    isScriptApproved(pkg: LockedPackage, script: string): boolean {
        return this.approveBuilds.check(pkg, script as any).approved;
    }
}

export function createSecurityManager(config: SecurityConfig): SecurityManager {
    return new SecurityManager(config);
}

// Re-export types
export type {
    SecurityConfig,
    SecurityCheckResult,
    LockdownCheckResult,
    LockdownViolation,
    ApprovedBuildsRecord,
    LifecycleScriptCheck,
} from './types.js';

// Re-export classes
export { MinimumReleaseAgeChecker } from './minimumReleaseAge.js';
export { ApproveBuildsManager } from './approveBuilds.js';
export { LockdownChecker } from './lockdown.js';

// Re-export factory functions
export { createMinimumReleaseAgeChecker } from './minimumReleaseAge.js';
export { createApproveBuildsManager } from './approveBuilds.js';
export { createLockdownChecker } from './lockdown.js';
