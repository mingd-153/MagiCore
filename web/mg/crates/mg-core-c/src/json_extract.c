#include "mg_json.h"
#include <string.h>
#include <stdlib.h>

/* ── Skip helpers ── */

/* Skip over a JSON value: string, number, object, array, true, false, null.
 * Returns pointer to first character after the value, or NULL on error.
 */
static const char* skip_json_value(const char* p) {
    if (!p) return NULL;
    switch (*p) {
    case '"': {
        p++;
        while (*p) {
            if (*p == '\\') { if (*(p+1)) p += 2; else return NULL; }
            else if (*p == '"') { return p + 1; }
            else p++;
        }
        return NULL;
    }
    case '{': {
        int depth = 1;
        p++;
        while (*p && depth > 0) {
            if (*p == '{') depth++;
            else if (*p == '}') depth--;
            else if (*p == '"') {
                p++;
                while (*p) {
                    if (*p == '\\') { if (*(p+1)) p += 2; else return NULL; }
                    else if (*p == '"') break;
                    else p++;
                }
                if (!*p) return NULL;
            }
            p++;
        }
        return depth == 0 ? p : NULL;
    }
    case '[': {
        int depth = 1;
        p++;
        while (*p && depth > 0) {
            if (*p == '[') depth++;
            else if (*p == ']') depth--;
            else if (*p == '"') {
                p++;
                while (*p) {
                    if (*p == '\\') { if (*(p+1)) p += 2; else return NULL; }
                    else if (*p == '"') break;
                    else p++;
                }
                if (!*p) return NULL;
            }
            p++;
        }
        return depth == 0 ? p : NULL;
    }
    case 't': /* true */
        if (p[1] == 'r' && p[2] == 'u' && p[3] == 'e') return p + 4;
        return NULL;
    case 'f': /* false */
        if (p[1] == 'a' && p[2] == 'l' && p[3] == 's' && p[4] == 'e') return p + 5;
        return NULL;
    case 'n': /* null */
        if (p[1] == 'u' && p[2] == 'l' && p[3] == 'l') return p + 4;
        return NULL;
    default: /* number */
        if (*p == '-' || (*p >= '0' && *p <= '9')) {
            p++;
            while (*p >= '0' && *p <= '9') p++;
            if (*p == '.') { p++; while (*p >= '0' && *p <= '9') p++; }
            if (*p == 'e' || *p == 'E') {
                p++;
                if (*p == '+' || *p == '-') p++;
                while (*p >= '0' && *p <= '9') p++;
            }
            return p;
        }
        return NULL;
    }
}

/* Skip whitespace */
static const char* skip_ws(const char* p) {
    if (!p) return NULL;
    while (*p == ' ' || *p == '\t' || *p == '\n' || *p == '\r') p++;
    return p;
}

/* Find a key in a JSON object and position after the colon.
 * The object must start just after '{'.
 * Returns pointer to value start, or NULL if not found.
 */
static const char* find_key(const char* p, const char* key, size_t key_len) {
    if (!p) return NULL;
    while (*p) {
        p = skip_ws(p);
        if (*p == '}') return NULL;
        if (*p == '"') {
            p++; /* skip opening quote */
            /* Compare key */
            size_t i;
            for (i = 0; i < key_len && *p; i++, p++) {
                if (*p != key[i]) break;
            }
            if (i == key_len && *p == '"') {
                /* Key matches! Skip to value */
                p++; /* closing quote */
                p = skip_ws(p);
                if (*p == ':') {
                    p = skip_ws(p + 1);
                    return p; /* points to value */
                }
                return NULL;
            }
            /* Skip to end of this key */
            while (*p && *p != '"') {
                if (*p == '\\') { if (*(p+1)) p++; }
                p++;
            }
            if (*p == '"') p++; /* closing quote */
            p = skip_ws(p);
            if (*p == ':') p++;
            p = skip_ws(p);
            p = skip_json_value(p);
            if (!p) return NULL;
            p = skip_ws(p);
            if (*p == ',') p++;
        } else {
            return NULL;
        }
    }
    return NULL;
}

/* ── Public API ── */

int mg_json_get_string(const char* json, const char* key, char* out, size_t out_len) {
    if (!json || !key || !out || out_len < 1) return -1;

    const char* p = skip_ws(json);
    if (!p || *p != '{') return -1;
    p++; /* skip '{' */

    size_t key_len = strlen(key);

    /* Support dotted paths: "dist.tarball" */
    const char* dot = strchr(key, '.');
    if (dot) {
        size_t first_len = (size_t)(dot - key);
        p = find_key(p, key, first_len);
        if (!p) return -1;
        /* Navigate into value (must be object) */
        p = skip_ws(p);
        if (*p != '{') return -1;
        p++; /* skip '{' */
        return mg_json_get_string(p, dot + 1, out, out_len);
    }

    p = find_key(p, key, key_len);
    if (!p) return -1;

    p = skip_ws(p);
    if (*p != '"') return -1;
    p++; /* skip opening quote */

    size_t i = 0;
    while (*p && *p != '"' && i < out_len - 1) {
        if (*p == '\\') {
            p++;
            if (*p) {
                out[i++] = *p;
                p++;
            }
        } else {
            out[i++] = *p;
            p++;
        }
    }
    out[i] = '\0';

    return (*p == '"') ? 0 : -1;
}

int mg_json_get_int(const char* json, const char* key, int* out) {
    char buf[32];
    if (mg_json_get_string(json, key, buf, sizeof(buf)) != 0) return -1;
    char* end = NULL;
    long val = strtol(buf, &end, 10);
    if (end == buf || *end != '\0') return -1;
    *out = (int)val;
    return 0;
}

int mg_json_object_for_each(const char* json, mg_json_field_cb cb, void* ctx) {
    if (!json || !cb) return -1;
    const char* p = skip_ws(json);
    if (!p || *p != '{') return -1;
    p++; /* skip '{' */

    while (*p) {
        p = skip_ws(p);
        if (*p == '}') return 0;
        if (*p != '"') return -1;

        p++; /* skip opening quote */
        const char* key_start = p;
        while (*p && *p != '"') {
            if (*p == '\\') { if (*(p+1)) p++; }
            p++;
        }
        if (*p != '"') return -1;
        const char* key_end = p;
        p++; /* closing quote */

        p = skip_ws(p);
        if (*p != ':') return -1;
        p = skip_ws(p + 1);

        const char* val_start = p;
        p = skip_json_value(p);
        if (!p) return -1;
        const char* val_end = p;

        int ret = cb(key_start, (size_t)(key_end - key_start),
                     val_start, (size_t)(val_end - val_start), ctx);
        if (ret != 0) return ret;

        p = skip_ws(p);
        if (*p == ',') p++;
        else if (*p == '}') return 0;
    }
    return 0;
}

/* ── Version iteration callbacks ── */

int mg_json_iterate_versions(const char* json, mg_json_field_cb cb, void* ctx) {
    if (!json || !cb) return -1;
    const char* p = skip_ws(json);
    if (!p || *p != '{') return -1;
    p++; /* skip '{' */

    p = find_key(p, "versions", 8);
    if (!p) return -1;
    p = skip_ws(p);
    if (*p != '{') return -1;
    p++; /* skip '{' */

    while (*p) {
        p = skip_ws(p);
        if (*p == '}') return 0;
        if (*p != '"') return -1;

        p++; /* skip opening quote */
        const char* key_start = p;
        while (*p && *p != '"') {
            if (*p == '\\') { if (*(p+1)) p++; }
            p++;
        }
        if (*p != '"') return -1;
        size_t key_len = (size_t)(p - key_start);

        /* Find the end of this version's value (skip entire version object) */
        p++; /* closing quote */
        p = skip_ws(p);
        if (*p != ':') return -1;
        p = skip_ws(p + 1);

        /* Skip the value (the version object) */
        p = skip_json_value(p);
        if (!p) return -1;

        int ret = cb(key_start, key_len, NULL, 0, ctx);
        if (ret != 0) return ret;

        p = skip_ws(p);
        if (*p == ',') p++;
        else if (*p == '}') return 0;
    }
    return 0;
}

int mg_json_iterate_deps(const char* json, const char* version,
                          mg_json_field_cb cb, void* ctx) {
    if (!json || !version || !cb) return -1;
    const char* p = skip_ws(json);
    if (!p || *p != '{') return -1;
    p++; /* skip '{' */

    /* Find "versions" */
    p = find_key(p, "versions", 8);
    if (!p) return -1;
    p = skip_ws(p);
    if (*p != '{') return -1;
    p++; /* skip '{' */

    /* Find the requested version key */
    p = find_key(p, version, strlen(version));
    if (!p) return -1;
    p = skip_ws(p);
    if (*p != '{') return -1;
    p++; /* skip '{' */

    /* Find "dependencies" inside this version */
    p = find_key(p, "dependencies", 12);
    if (!p) return -1;
    p = skip_ws(p);
    if (*p != '{') return -1;
    p++; /* skip '{' */

    return mg_json_object_for_each(p - 1, cb, ctx);
}
