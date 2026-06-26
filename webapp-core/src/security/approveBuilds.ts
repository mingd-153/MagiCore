// Approve Builds Manager
// Deny lifecycle scripts by default, require explicit approval

import { LockedPackage } from '../types/index.js';
import { SecurityConfig, ApprovedBuildsRecord, LifecycleScriptCheck } from './types.js';

const LIFECYCLE_SCRIPTS = [
    'prepare',
    'preinstall',
    'postinstall',
    'prepublish',
    'prepack',
] as const;
type LifecycleScript = (typeof LIFECYCLE_SCRIPTS)[number];

export class ApproveBuildsManager {
    private approvedBuilds: ApprovedBuildsRecord = {};

    constructor(private config: SecurityConfig) {}

    /**
     * Load approved builds from lockfile
     */
    loadFromLockfile(approvedBuilds?: ApprovedBuildsRecord): void {
        this.approvedBuilds = approvedBuilds || {};
        console.log(
            `[SECURITY] 📋 Loaded approve-builds allowlist: ${Object.keys(this.approvedBuilds).length} packages`
        );
    }

    /**
     * Get current approved builds record
     */
    getApprovedBuilds(): ApprovedBuildsRecord {
        return { ...this.approvedBuilds };
    }

    /**
     * Check if a lifecycle script is approved for a package
     */
    check(pkg: LockedPackage, script: LifecycleScript): LifecycleScriptCheck {
        const packageKey = `${pkg.name}@${pkg.version}`;
        const approvedScripts = this.approvedBuilds[packageKey] || [];
        const approved = approvedScripts.includes(script);

        if (!this.config.approveBuilds) {
            console.log(
                `[SECURITY] ⚠️ approveBuilds DISABLED - allowing ${script} for ${packageKey}`
            );
            return { packageName: pkg.name, version: pkg.version, script, approved: true };
        }

        if (approved) {
            console.log(`[SECURITY] ✅ APPROVED lifecycle script: ${script} for ${packageKey}`);
        } else {
            console.warn(
                `[SECURITY] 🚫 BLOCKED lifecycle script: ${script} for ${packageKey} ` +
                    `(not in allowlist). Run: megagate security approve-builds ${pkg.name} --script ${script}`
            );
        }

        return { packageName: pkg.name, version: pkg.version, script, approved };
    }

    /**
     * Approve a lifecycle script for a package (persist to lockfile)
     */
    approve(pkg: LockedPackage, script: LifecycleScript): void {
        const packageKey = `${pkg.name}@${pkg.version}`;
        if (!this.approvedBuilds[packageKey]) {
            this.approvedBuilds[packageKey] = [];
        }
        if (!this.approvedBuilds[packageKey].includes(script)) {
            this.approvedBuilds[packageKey].push(script);
            console.log(`[SECURITY] ✅ ADDED to allowlist: ${script} for ${packageKey}`);
        }
    }

    /**
     * Check all lifecycle scripts for a package
     */
    checkAll(pkg: LockedPackage): LifecycleScriptCheck[] {
        return LIFECYCLE_SCRIPTS.map((script) => this.check(pkg, script));
    }

    /**
     * Get packages that have unapproved scripts (for reporting)
     */
    getUnapprovedPackages(packages: LockedPackage[]): LockedPackage[] {
        return packages.filter((pkg) => {
            return LIFECYCLE_SCRIPTS.some((script) => !this.check(pkg, script).approved);
        });
    }
}

export function createApproveBuildsManager(config: SecurityConfig): ApproveBuildsManager {
    return new ApproveBuildsManager(config);
}
