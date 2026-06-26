// Dependency Graph Builder
// Core resolution logic with security hooks

import semver from 'semver';
import { 
  PackageRef, 
  MegagateConfig,
  SecurityCheckResult,
  LockdownCheckResult 
} from '../types/index.js';

export interface DependencyNode {
  name: string;
  range: string;
  resolved?: ResolvedDependency;
  dependencies: Map<string, DependencyNode>;
  dependents: Set<string>;
  // Security flags
  securityCheck?: SecurityCheckResult;
  lockdownCheck?: LockdownCheckResult;
}

export interface Conflict {
  name: string;
  versions: Array<{ version: string; requiredBy: string[] }>;
  resolution?: 'hoist' | 'duplicate' | 'error';
}

export interface ResolvedDependency {
  name: string;
  version: string;
  integrity: string;
  resolved: string;
  size: number;
  dependencies?: Record<string, string>;
  optionalDependencies?: Record<string, string>;
  peerDependencies?: Record<string, string>;
  bin?: Record<string, string>;
  engines?: Record<string, string>;
  scripts?: Record<string, string>;
  publishTime?: string;
  approvedBuilds?: string[];
}

export class DependencyGraph {
  private nodes = new Map<string, DependencyNode>();
  private config: MegagateConfig;
  private securityEnabled: boolean = false;

  constructor(config: MegagateConfig) {
    this.config = config;
  }

  enableSecurity() {
    this.securityEnabled = true;
  }

  addNode(name: string, range: string): DependencyNode {
    const key = `${name}@${range}`;
    if (!this.nodes.has(key)) {
      this.nodes.set(key, {
        name,
        range,
        dependencies: new Map(),
        dependents: new Set(),
      });
    }
    return this.nodes.get(key)!;
  }

  getNode(name: string, version: string): DependencyNode | undefined {
    const key = `${name}@${version}`;
    return this.nodes.get(key);
  }

  addEdge(parent: DependencyNode, child: DependencyNode): void {
    parent.dependencies.set(child.name, child);
    child.dependents.add(parent.name);
  }

  detectConflicts(): Conflict[] {
    const conflicts: Conflict[] = [];
    const nameToVersions = new Map<string, Set<string>>();

    for (const node of this.nodes.values()) {
      if (!nameToVersions.has(node.name)) {
        nameToVersions.set(node.name, new Set());
      }
      nameToVersions.get(node.name)!.add(node.range);
    }

    for (const [name, ranges] of nameToVersions.entries()) {
      if (ranges.size > 1) {
        const versions = Array.from(ranges).map(range => {
          const node = this.findNodeByRange(name, range);
          return {
            version: node?.resolved?.version || range,
            requiredBy: Array.from(node?.dependents || []),
          };
        });
        conflicts.push({ name, versions });
      }
    }

    return conflicts;
  }

  private findNodeByRange(name: string, range: string): DependencyNode | undefined {
    const key = `${name}@${range}`;
    return this.nodes.get(key);
  }

  getResolutionOrder(): DependencyNode[] {
    // Topological sort
    const visited = new Set<string>();
    const order: DependencyNode[] = [];

    const visit = (node: DependencyNode) => {
      const key = `${node.name}@${node.resolved?.version || node.range}`;
      if (visited.has(key)) return;
      visited.add(key);

      for (const dep of node.dependencies.values()) {
        visit(dep);
      }
      order.push(node);
    };

    for (const node of this.nodes.values()) {
      if (node.dependents.size === 0) { // Root nodes (no dependents)
        visit(node);
      }
    }

    return order;
  }

  setSecurityResult(nodeName: string, version: string, result: SecurityCheckResult): void {
    const key = `${nodeName}@${version}`;
    const node = this.nodes.get(key);
    if (node) node.securityCheck = result;
  }

  setLockdownResult(nodeName: string, version: string, result: any): void {
    const key = `${nodeName}@${version}`;
    const node = this.nodes.get(key);
    if (node) node.lockdownCheck = result;
  }
}
