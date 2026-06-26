// MegaGate Store Abstraction - Pluggable backends
// Interface for content-addressable store

import { Readable } from 'stream';
import { 
  PackageRef, 
  IntegrityInfo, 
  PackageManifest, 
  PackageMetadata, 
  PruneResult,
  MegagateConfig 
} from '../types/index.js';

export type { 
  PackageRef, 
  IntegrityInfo, 
  PackageManifest, 
  PackageMetadata, 
  PruneResult,
  MegagateConfig 
} from '../types/index.js';

export interface StoreBackend {
  init(config: MegagateConfig): Promise<void>;
  exists(pkg: PackageRef): Promise<boolean>;
  getPath(pkg: PackageRef): string;
  writeTarball(pkg: PackageRef, stream: Readable): Promise<IntegrityInfo>;
  readTarball(pkg: PackageRef): Promise<Readable>;
  writeManifest(pkg: PackageRef, manifest: PackageManifest): Promise<void>;
  readManifest(pkg: PackageRef): Promise<PackageManifest | null>;
  writeMetadata(pkg: PackageRef, meta: PackageMetadata): Promise<void>;
  readMetadata(pkg: PackageRef): Promise<PackageMetadata | null>;
  createHardlink(pkg: PackageRef, target: string): Promise<void>;
  createSymlink(pkg: PackageRef, target: string): Promise<void>;
  remove(pkg: PackageRef): Promise<void>;
  prune(referenced: Set<string>): Promise<PruneResult>;
  verifyIntegrity(pkg: PackageRef): Promise<boolean>;
}

export interface StoreBackendFactory {
  create(config: MegagateConfig): StoreBackend;
}

export { createFsStoreBackend, FsStoreBackend } from './fsBackend.js';
