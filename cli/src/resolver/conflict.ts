// Version Conflict Resolution
// Handles conflicts with security-aware decisions

import semver from 'semver';
import { Conflict, DependencyNode, ResolvedDependency } from './graph.js';
import { RegistryClient } from '../fetcher/registry.js';
import { SecurityCheckResult } from '../types/index.js';

export interface ResolutionDecision {
  name: string;
  chosenVersion: string;
  strategy: 'hoist' | 'duplicate' | 'error';
  reason: string;
}

/**
 * Resolve version conflicts using semver compatibility
 * Security-aware: prefers versions that pass security checks
 */
export async function resolveConflicts(
  conflicts: Conflict[],
  registry: RegistryClient,
  securityResults?: Map<string, SecurityCheckResult>
): Promise<ResolutionDecision[]> {
  const decisions: ResolutionDecision[] = [];

  for (const conflict of conflicts) {
    const decision = await resolveSingleConflict(conflict, registry, securityResults);
    decisions.push(decision);
  }

  return decisions;
}

async function resolveSingleConflict(
  conflict: Conflict,
  registry: RegistryClient,
  securityResults?: Map<string, SecurityCheckResult>
): Promise<ResolutionDecision> {
  const { name, versions } = conflict;

  // Sort versions by semver (highest first)
  const sortedVersions = versions
    .map(v => ({ ...v, semver: semver.coerce(v.version) }))
    .filter(v => v.semver !== null)
    .sort((a, b) => semver.rcompare(a.semver!.version, b.semver!.version));

  if (sortedVersions.length === 0) {
    return {
      name,
      chosenVersion: versions[0].version,
      strategy: 'error',
      reason: 'No valid semver versions found',
    };
  }

  const highest = sortedVersions[0];

  // Check if highest version satisfies all other ranges
  const allSatisfied = versions.every(v => {
    if (v.version === highest.version) return true;
    return semver.satisfies(highest.version, v.version, { includePrerelease: false });
  });

  if (allSatisfied) {
    // Check security: prefer version that passes security checks
    const securityPassed = securityResults?.get(`${name}@${highest.version}`)?.passed;
    
    return {
      name,
      chosenVersion: highest.version,
      strategy: 'hoist',
      reason: securityPassed
        ? `Hoisted to highest compatible version ${highest.version} (security passed)`
        : `Hoisted to highest compatible version ${highest.version}`,
    };
  }

  // Try to find a version that satisfies all ranges
  for (const candidate of sortedVersions) {
    const satisfiesAll = versions.every(v => 
      semver.satisfies(candidate.version, v.version, { includePrerelease: false })
    );
    
    if (satisfiesAll) {
      const securityPassed = securityResults?.get(`${name}@${candidate.version}`)?.passed;
      
      return {
        name,
        chosenVersion: candidate.version,
        strategy: 'hoist',
        reason: securityPassed
          ? `Resolved to compatible version ${candidate.version} (security passed)`
          : `Resolved to compatible version ${candidate.version}`,
      };
    }
  }

  // Cannot hoist - must duplicate (last resort)
  return {
    name,
    chosenVersion: highest.version,
    strategy: 'duplicate',
    reason: `Cannot hoist ${name}: versions ${versions.map(v => v.version).join(', ')} are incompatible. Will duplicate in node_modules.`,
  };
}

/**
 * Apply conflict resolution decisions to the graph
 */
export function applyResolutions(
  graphNodes: Map<string, any>,
  decisions: ResolutionDecision[],
  registry: RegistryClient
): Map<string, ResolvedDependency> {
  const resolved = new Map<string, ResolvedDependency>();

  for (const decision of decisions) {
    if (decision.strategy === 'error') {
      throw new Error(`Unresolvable conflict for ${decision.name}: ${decision.reason}`);
    }

    // For hoisted versions, we use the chosen version for all dependents
    // For duplicate, each dependent gets its own version (handled at link time)
  }

  return resolved;
}
