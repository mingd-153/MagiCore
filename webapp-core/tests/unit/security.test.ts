import { describe, it, expect, beforeEach } from 'vitest';
import { LockedPackage } from '../../src/types/index.js';
import {
    MinimumReleaseAgeChecker,
    createMinimumReleaseAgeChecker,
    ApproveBuildsManager,
    createApproveBuildsManager,
    LockdownChecker,
    createLockdownChecker,
    SecurityManager,
    createSecurityManager,
} from '../../src/security/index.js';

describe('Security Module', () => {
    const baseConfig = {
        minimumReleaseAgeHours: 24,
        approveBuilds: true,
        lockdownMode: false,
    };

    const oldPackage: LockedPackage = {
        name: 'old-pkg',
        version: '1.0.0',
        integrity: 'sha512-old',
        resolved: 'https://registry.npmjs.org/old-pkg/-/old-pkg-1.0.0.tgz',
        size: 1000,
        publishTime: '2024-01-01T00:00:00.000Z',
    };

    const newPackage: LockedPackage = {
        name: 'new-pkg',
        version: '1.0.0',
        integrity: 'sha512-new',
        resolved: 'https://registry.npmjs.org/new-pkg/-/new-pkg-1.0.0.tgz',
        size: 1000,
        publishTime: new Date(Date.now() - 2 * 60 * 60 * 1000).toISOString(), // 2 hours ago
    };

    const packageWithGyp: LockedPackage = {
        name: 'native-pkg',
        version: '1.0.0',
        integrity: 'sha512-native',
        resolved: 'https://registry.npmjs.org/native-pkg/-/native-pkg-1.0.0.tgz',
        size: 1000,
        publishTime: '2024-01-01T00:00:00.000Z',
    };

    describe('MinimumReleaseAgeChecker', () => {
        let checker: MinimumReleaseAgeChecker;

        beforeEach(() => {
            checker = createMinimumReleaseAgeChecker(baseConfig);
        });

        it('allows packages older than 24h', () => {
            const result = checker.check(oldPackage);
            expect(result.allowed).toBe(true);
        });

        it('blocks packages newer than 24h', () => {
            const result = checker.check(newPackage);
            expect(result.allowed).toBe(false);
            expect(result.reason).toContain('minimum is 24h');
        });

        it('blocks packages without publishTime', () => {
            const pkg: LockedPackage = { ...oldPackage, publishTime: undefined as any };
            const result = checker.check(pkg);
            expect(result.allowed).toBe(false);
            expect(result.reason).toContain('missing publishTime');
        });

        it('checkAll separates passed and blocked', () => {
            const { passed, blocked } = checker.checkAll([oldPackage, newPackage]);
            expect(passed.length).toBe(1);
            expect(blocked.length).toBe(1);
            expect(passed[0].name).toBe('old-pkg');
            expect(blocked[0].name).toBe('new-pkg');
        });
    });

    describe('ApproveBuildsManager', () => {
        let manager: ApproveBuildsManager;

        beforeEach(() => {
            manager = createApproveBuildsManager(baseConfig);
        });

        it('blocks unapproved scripts by default', () => {
            const result = manager.check(oldPackage, 'postinstall');
            expect(result.approved).toBe(false);
        });

        it('allows scripts when approveBuilds is disabled', () => {
            const disabledConfig = { ...baseConfig, approveBuilds: false };
            const disabledManager = createApproveBuildsManager(disabledConfig);
            const result = disabledManager.check(oldPackage, 'postinstall');
            expect(result.approved).toBe(true);
        });

        it('approves script and persists', () => {
            manager.approve(oldPackage, 'postinstall');
            const result = manager.check(oldPackage, 'postinstall');
            expect(result.approved).toBe(true);
        });

        it('loads from lockfile', () => {
            manager.loadFromLockfile({ 'old-pkg@1.0.0': ['postinstall'] });
            const result = manager.check(oldPackage, 'postinstall');
            expect(result.approved).toBe(true);
        });

        it('checkAll returns all lifecycle scripts', () => {
            const results = manager.checkAll(oldPackage);
            expect(results.length).toBe(5); // prepare, preinstall, postinstall, prepublish, prepack
        });
    });

    describe('LockdownChecker', () => {
        let checker: LockdownChecker;

        beforeEach(() => {
            checker = createLockdownChecker(baseConfig);
        });

        it('allows all when lockdownMode is false', () => {
            const result = checker.check(packageWithGyp);
            expect(result.allowed).toBe(true);
            expect(result.violations.length).toBe(0);
        });

        it('blocks native addons when lockdownMode is true', () => {
            const lockdownConfig = { ...baseConfig, lockdownMode: true };
            const lockdownChecker = createLockdownChecker(lockdownConfig);
            const result = lockdownChecker.check(packageWithGyp);
            expect(result.allowed).toBe(false);
            expect(result.violations.some((v) => v.type === 'native-addon')).toBe(true);
        });

        it('checkAll separates passed and failed', () => {
            const lockdownConfig = { ...baseConfig, lockdownMode: true };
            const lockdownChecker = createLockdownChecker(lockdownConfig);
            const { passed, failed } = lockdownChecker.checkAll([oldPackage, packageWithGyp]);
            expect(passed.length).toBe(1);
            expect(failed.length).toBe(1);
        });
    });

    describe('SecurityManager', () => {
        let manager: SecurityManager;

        beforeEach(() => {
            manager = createSecurityManager(baseConfig);
        });

        it('checks package against all policies', () => {
            const result = manager.checkPackage(oldPackage);
            expect(result.allowed).toBe(true);
            expect(result.minimumReleaseAge.allowed).toBe(true);
            expect(result.lockdown.allowed).toBe(true);
        });

        it('blocks package failing any policy', () => {
            const result = manager.checkPackage(newPackage);
            expect(result.allowed).toBe(false);
            expect(result.minimumReleaseAge.allowed).toBe(false);
        });

        it('checkAllPackages returns summary', () => {
            const { allowed, blocked, summary } = manager.checkAllPackages([
                oldPackage,
                newPackage,
            ]);
            expect(summary.total).toBe(2);
            expect(summary.passed).toBe(1);
            expect(summary.blockedByAge).toBe(1);
            expect(summary.blockedByLockdown).toBe(0);
        });

        it('approveScript works', () => {
            manager.approveScript(oldPackage, 'postinstall');
            const approved = manager.isScriptApproved(oldPackage, 'postinstall');
            expect(approved).toBe(true);
        });
    });
});
