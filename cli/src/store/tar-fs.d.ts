declare module 'tar-fs' {
    import { Readable, Writable } from 'stream';

    export interface ExtractOptions {
        strip?: number;
        map?: (header: any) => any;
        fmode?: string;
        dmode?: string;
        readable?: boolean;
        writable?: boolean;
        ignore?: (name: string) => boolean;
    }

    export interface PackOptions {
        entries?: Array<{ name: string; size?: number; mode?: number; mtime?: Date }>;
        finish?: () => void;
        fs?: any;
        ignore?: (name: string) => boolean;
        mapStream?: (stream: any) => any;
        map?: (header: any) => any;
        dereference?: boolean;
        strict?: boolean;
        umask?: number;
        dmode?: number;
        fmode?: number;
        readable?: boolean;
        writable?: boolean;
        strip?: number;
        finalize?: boolean;
        pack?: any;
    }

    export function extract(dest: string, options?: ExtractOptions): Writable;
    export function pack(cwd: string, options?: PackOptions): Readable;
}
