#ifndef MG_JSON_H
#define MG_JSON_H

#include <stddef.h>
#include <stdbool.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ── Lightweight JSON field extraction ──
 *
 * Intended for npm registry response parsing.
 * These functions skip/skim JSON rather than building a full parse tree.
 * They handle flat and one-deep nested field paths (e.g. "dist.tarball").
 */

/* Extract a string field value by key path.
 * key can be simple ("name") or dotted ("dist.tarball").
 * Returns 0 on success, -1 if field not found or not a string.
 * out is null-terminated on success.
 */
int mg_json_get_string(const char* json, const char* key,
                       char* out, size_t out_len);

/* Extract an integer field value by key path.
 * Returns 0 on success, -1 if not found or not integer.
 */
int mg_json_get_int(const char* json, const char* key, int* out);

/* Callback invoked for each key-value pair in an object.
 * Return 0 to continue, non-zero to stop iteration.
 */
typedef int (*mg_json_field_cb)(const char* key, size_t key_len,
                                const char* val, size_t val_len,
                                void* ctx);

/* Iterate all top-level key-value pairs in a JSON object.
 * Stops early if callback returns non-zero.
 * Returns 0 on success, -1 on parse error.
 */
int mg_json_object_for_each(const char* json, mg_json_field_cb cb, void* ctx);

/* Find the "versions" object inside npm package metadata and iterate
 * its keys (version strings). Each key is passed to cb with val=val_len=0.
 * Returns 0 on success, -1 if "versions" not found or not an object.
 */
int mg_json_iterate_versions(const char* json, mg_json_field_cb cb, void* ctx);

/* Find a specific version entry inside the "versions" object, then
 * iterate its "dependencies" object key-value pairs.
 * Returns 0 on success, -1 if version or deps not found.
 */
int mg_json_iterate_deps(const char* json, const char* version,
                          mg_json_field_cb cb, void* ctx);

#ifdef __cplusplus
}
#endif

#endif /* MG_JSON_H */
