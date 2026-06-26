// Security types

export interface SecurityConfig {
  minimumReleaseAgeHours: number;
  approveBuilds: boolean;
  lockdownMode: boolean;
}

export interface SecurityCheckResult {
  allowed: boolean;
  reason?: string;
  blockedAt?: string;
}

export interface LifecycleScriptCheck {
  packageName: string;
  version: string;
  script: 'prepare' | 'preinstall' | 'postinstall' | 'prepublish' | 'prepack';
  approved: boolean;
}

export interface LockdownCheckResult {
  allowed: boolean;
  violations: LockdownViolation[];
}

export interface LockdownViolation {
  type: 'native-addon' | 'eval-usage' | 'function-constructor' | 'unsafe-sideeffects';
  file: string;
  message: string;
}

export interface ApprovedBuildsRecord {
  [packageKey: string]: string[]; // package@version -> ['postinstall', 'prepare']
}
