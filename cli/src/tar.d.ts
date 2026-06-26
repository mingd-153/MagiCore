declare module 'tar' {
    export interface ExtractOptions {
        C?: string;
        strip?: number;
        filter?: (entry: any) => boolean;
        onentry?: (entry: any) => void;
    }

    export function x(options: ExtractOptions): NodeJS.ReadWriteStream;

    export namespace x {
        export { Readable } from 'stream';
    }
}
