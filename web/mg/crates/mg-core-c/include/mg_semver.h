#ifndef MG_SEMVER_H
#define MG_SEMVER_H

#include <stdbool.h>
#include <stdint.h>
#include <stddef.h>
#include <string.h>
#include <ctype.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ── Version ── */
#define MG_PRERELEASE_MAX 63

typedef struct {
    uint64_t major;
    uint64_t minor;
    uint64_t patch;
    char prerelease[MG_PRERELEASE_MAX + 1];
    int prerelease_len;
} mg_version_t;

/* Parse "X.Y.Z" or "X.Y.Z-pre.id" or "X.Y.Z+build" or "X.Y.Z-pre.id+build"
 * Returns 0 on success, -1 on parse error.
 */
int mg_version_parse(const char* s, mg_version_t* v);

/* Compare two versions per semver 2.0.0.
 * Returns -1 if a < b, 0 if equal, 1 if a > b.
 * Pre-release < release. Numeric identifiers compare numerically,
 * string identifiers lexicographically, numeric < string.
 */
int mg_version_cmp(const mg_version_t* a, const mg_version_t* b);

/* Format version to string. Returns number of chars written (not counting null).
 * Returns -1 if buffer too small.
 */
int mg_version_format(const mg_version_t* v, char* out, size_t out_len);

/* ── Range ── */
typedef enum {
    MG_RANGE_EXACT,
    MG_RANGE_CARET,
    MG_RANGE_TILDE,
    MG_RANGE_GTE,
    MG_RANGE_GT,
    MG_RANGE_LTE,
    MG_RANGE_LT,
    MG_RANGE_STAR,
    MG_RANGE_AND,
    MG_RANGE_OR,
    MG_RANGE_INVALID,
} mg_range_type_t;

typedef struct mg_range {
    mg_range_type_t type;
    mg_version_t min;
    mg_version_t max;
    struct mg_range* sub_left;
    struct mg_range* sub_right;
} mg_range_t;

/* Parse a range string into mg_range_t.
 * Supports: ^x.y.z ~x.y.z >=x.y.z >x.y.z <=x.y.z <x.y.z x.y.z * >=a.b.c <d.e.f
 *           ^x.y.z || ^a.b.c
 * Returns 0 on success, -1 on error.
 * NOTE: For OR/AND ranges, sub_left/sub_right point to static internal storage.
 * The returned range is valid until the next call to mg_range_parse.
 */
int mg_range_parse(const char* s, mg_range_t* r);

/* Returns true if range r contains version v.
 * Follows npm semver rules for pre-release matching:
 * a pre-release version only matches if its base (without pre-release) also
 * falls within the range.
 */
bool mg_range_contains(const mg_range_t* r, const mg_version_t* v);

/* Clean up any internal state (call once at program exit if needed). */
void mg_semver_cleanup(void);

#ifdef __cplusplus
}
#endif

#endif /* MG_SEMVER_H */
