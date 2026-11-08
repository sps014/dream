/** Utilities for encoding and decoding JavaScript JSON values. */
interface JSON {
    /** Encodes a JavaScript value as JSON text. */
    stringify(value: any): string;
    /** Decodes JSON text into a JavaScript value. Invalid JSON throws in JavaScript. */
    parse(text: string): any;
}
declare var JSON: JSON;
