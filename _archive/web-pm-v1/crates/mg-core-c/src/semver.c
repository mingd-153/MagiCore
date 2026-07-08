#include "mg_semver.h"
#include <stdlib.h>
#include <stdio.h>

/* ── Internal helpers ── */

static int parse_uint64(const char** s, uint64_t* out) {
    if (!isdigit((unsigned char)**s)) return -1;
    *out = 0;
    while (isdigit((unsigned char)**s)) {
        *out = (*out * 10) + (unsigned)(**s - '0');
        (*s)++;
    }
    return 0;
}

/* Compare two pre-release identifier strings using semver rules.
 * Each string may contain dot-separated identifiers.
 * Returns -1/0/1.
 */
static int cmp_prerelease(const char* a, size_t a_len,
                           const char* b, size_t b_len) {
    if (a_len == 0 && b_len == 0) return 0;
    if (a_len == 0) return -1;  /* no pre-release > pre-release */
    if (b_len == 0) return 1;

    /* Tokenize on '.' */
    const char* ap = a;
    const char* bp = b;
    const char* a_end = a + a_len;
    const char* b_end = b + b_len;

    while (ap < a_end && bp < b_end) {
        /* Find next dot or end */
        const char* adot = (const char*)memchr(ap, '.', (size_t)(a_end - ap));
        const char* bdot = (const char*)memchr(bp, '.', (size_t)(b_end - bp));
        if (!adot) adot = a_end;
        if (!bdot) bdot = b_end;

        size_t a_seg_len = (size_t)(adot - ap);
        size_t b_seg_len = (size_t)(bdot - bp);

        /* Determine if numeric */
        int a_is_num = 1, b_is_num = 1;
        for (size_t i = 0; i < a_seg_len; i++) {
            if (!isdigit((unsigned char)ap[i])) { a_is_num = 0; break; }
        }
        for (size_t i = 0; i < b_seg_len; i++) {
            if (!isdigit((unsigned char)bp[i])) { b_is_num = 0; break; }
        }

        int cmp;
        if (a_is_num && b_is_num) {
            /* Numeric comparison */
            uint64_t an = 0, bn = 0;
            for (size_t i = 0; i < a_seg_len; i++) an = an * 10 + (unsigned)(ap[i] - '0');
            for (size_t i = 0; i < b_seg_len; i++) bn = bn * 10 + (unsigned)(bp[i] - '0');
            if (an < bn) cmp = -1;
            else if (an > bn) cmp = 1;
            else cmp = 0;
        } else if (a_is_num && !b_is_num) {
            cmp = -1; /* numeric < string */
        } else if (!a_is_num && b_is_num) {
            cmp = 1;  /* string > numeric */
        } else {
            /* String comparison */
            size_t min_len = a_seg_len < b_seg_len ? a_seg_len : b_seg_len;
            cmp = memcmp(ap, bp, min_len);
            if (cmp == 0) {
                if (a_seg_len < b_seg_len) cmp = -1;
                else if (a_seg_len > b_seg_len) cmp = 1;
            }
        }
        if (cmp != 0) return cmp;

        ap = adot + 1;
        bp = bdot + 1;
    }

    /* Fewer fields < more fields if all equal so far */
    if (ap >= a_end && bp >= b_end) return 0;
    if (ap >= a_end) return -1;
    return 1;
}

/* ── Public API ── */

int mg_version_parse(const char* s, mg_version_t* v) {
    if (!s || !v) return -1;

    memset(v, 0, sizeof(*v));
    v->prerelease_len = -1;

    /* Skip leading whitespace */
    while (isspace((unsigned char)*s)) s++;
    if (!*s) return -1;

    const char* p = s;

    /* Parse major.minor.patch */
    if (parse_uint64(&p, &v->major) != 0) return -1;
    if (*p != '.') return -1;
    p++;
    if (parse_uint64(&p, &v->minor) != 0) return -1;
    if (*p != '.') return -1;
    p++;
    if (parse_uint64(&p, &v->patch) != 0) return -1;

    /* Optional pre-release */
    if (*p == '-') {
        p++;
        const char* pre_start = p;
        while (*p && *p != '+') p++;
        size_t pre_len = (size_t)(p - pre_start);
        if (pre_len > MG_PRERELEASE_MAX) return -1;
        memcpy(v->prerelease, pre_start, pre_len);
        v->prerelease[pre_len] = '\0';
        v->prerelease_len = (int)pre_len;
    }

    /* Optional build metadata (ignored in comparisons) */
    if (*p == '+') {
        p++;
        while (*p && !isspace((unsigned char)*p)) p++;
    }

    /* Skip trailing whitespace */
    while (isspace((unsigned char)*p)) p++;

    return (*p == '\0') ? 0 : -1;
}

int mg_version_cmp(const mg_version_t* a, const mg_version_t* b) {
    if (!a || !b) return 0;

    /* Compare major.minor.patch */
    if (a->major < b->major) return -1;
    if (a->major > b->major) return 1;
    if (a->minor < b->minor) return -1;
    if (a->minor > b->minor) return 1;
    if (a->patch < b->patch) return -1;
    if (a->patch > b->patch) return 1;

    /* Compare pre-release: no pre-release > pre-release */
    int a_has_pre = (a->prerelease_len >= 0);
    int b_has_pre = (b->prerelease_len >= 0);
    if (!a_has_pre && !b_has_pre) return 0;
    if (!a_has_pre) return 1;  /* a is release, b is pre-release => a > b */
    if (!b_has_pre) return -1; /* a is pre-release, b is release => a < b */

    return cmp_prerelease(a->prerelease, (size_t)a->prerelease_len,
                          b->prerelease, (size_t)b->prerelease_len);
}

int mg_version_format(const mg_version_t* v, char* out, size_t out_len) {
    if (!v || !out || out_len < 1) return -1;

    int n = snprintf(out, out_len, "%llu.%llu.%llu",
                     (unsigned long long)v->major,
                     (unsigned long long)v->minor,
                     (unsigned long long)v->patch);
    if (n < 0 || (size_t)n >= out_len) return -1;

    if (v->prerelease_len >= 0) {
        int m = snprintf(out + n, out_len - (size_t)n, "-%s", v->prerelease);
        if (m < 0 || (size_t)(n + m) >= out_len) return -1;
        n += m;
    }
    return n;
}

/* ── Range parsing (internal, re-entrant) ──
 *
 * To avoid heap allocation, we use a small static pool for sub-ranges
 * when parsing OR/AND ranges. This is safe because mg_range_parse is
 * typically called once per check and the result is used immediately.
 */
#define MG_RANGE_POOL_SIZE 8

static mg_range_t range_pool[MG_RANGE_POOL_SIZE];
static int pool_idx = 0;

static mg_range_t* alloc_sub_range(void) {
    if (pool_idx >= MG_RANGE_POOL_SIZE) return NULL;
    mg_range_t* r = &range_pool[pool_idx++];
    memset(r, 0, sizeof(*r));
    return r;
}

void mg_semver_cleanup(void) {
    pool_idx = 0;
    memset(range_pool, 0, sizeof(range_pool));
}

/* Parse a simple (non-OR, non-AND) range. E.g. "^1.2.3" or ">=1.0.0" */
static int parse_simple_range(const char* s, mg_range_t* r) {
    memset(r, 0, sizeof(*r));

    while (isspace((unsigned char)*s)) s++;

    if (*s == '*') {
        r->type = MG_RANGE_STAR;
        return 0;
    }

    if (*s == '^') {
        r->type = MG_RANGE_CARET;
        s++;
        while (isspace((unsigned char)*s)) s++;
        if (mg_version_parse(s, &r->min) != 0) return -1;
        r->max = r->min;
        r->max.major = r->min.major + 1;
        r->max.minor = 0;
        r->max.patch = 0;
        r->max.prerelease_len = -1;
        return 0;
    }

    if (*s == '~') {
        r->type = MG_RANGE_TILDE;
        s++;
        while (isspace((unsigned char)*s)) s++;
        if (mg_version_parse(s, &r->min) != 0) return -1;
        r->max = r->min;
        r->max.minor = r->min.minor + 1;
        r->max.patch = 0;
        r->max.prerelease_len = -1;
        return 0;
    }

    if (s[0] == '>' && s[1] == '=') {
        r->type = MG_RANGE_GTE;
        s += 2;
        while (isspace((unsigned char)*s)) s++;
        if (mg_version_parse(s, &r->min) != 0) return -1;
        return 0;
    }

    if (*s == '>') {
        r->type = MG_RANGE_GT;
        s++;
        while (isspace((unsigned char)*s)) s++;
        if (mg_version_parse(s, &r->min) != 0) return -1;
        return 0;
    }

    if (s[0] == '<' && s[1] == '=') {
        r->type = MG_RANGE_LTE;
        s += 2;
        while (isspace((unsigned char)*s)) s++;
        if (mg_version_parse(s, &r->min) != 0) return -1;
        return 0;
    }

    if (*s == '<') {
        r->type = MG_RANGE_LT;
        s++;
        while (isspace((unsigned char)*s)) s++;
        if (mg_version_parse(s, &r->min) != 0) return -1;
        return 0;
    }

    /* Try exact version */
    if (mg_version_parse(s, &r->min) == 0) {
        r->type = MG_RANGE_EXACT;
        return 0;
    }

    r->type = MG_RANGE_INVALID;
    return -1;
}

int mg_range_parse(const char* s, mg_range_t* r) {
    if (!s || !r) return -1;

    /* Reset pool */
    pool_idx = 0;

    while (isspace((unsigned char)*s)) s++;

    size_t len = strlen(s);

    /* Check for OR (||) */
    {
        const char* or_pos = strstr(s, "||");
        if (or_pos) {
            r->type = MG_RANGE_OR;
            r->sub_left = alloc_sub_range();
            r->sub_right = alloc_sub_range();
            if (!r->sub_left || !r->sub_right) return -1;

            /* Left side */
            size_t left_len = (size_t)(or_pos - s);
            char* left = (char*)malloc(left_len + 1);
            if (!left) return -1;
            memcpy(left, s, left_len);
            left[left_len] = '\0';
            int ret = parse_simple_range(left, r->sub_left);
            free(left);
            if (ret != 0) return -1;

            /* Right side */
            const char* right = or_pos + 2;
            while (isspace((unsigned char)*right)) right++;
            return parse_simple_range(right, r->sub_right);
        }
    }

    /* Check for AND (>=x <y pattern, or &&) */
    {
        /* Try "&&" delimiter */
        const char* and_pos = strstr(s, "&&");
        if (and_pos) {
            r->type = MG_RANGE_AND;
            r->sub_left = alloc_sub_range();
            r->sub_right = alloc_sub_range();
            if (!r->sub_left || !r->sub_right) return -1;

            size_t left_len = (size_t)(and_pos - s);
            char* left = (char*)malloc(left_len + 1);
            if (!left) return -1;
            memcpy(left, s, left_len);
            left[left_len] = '\0';
            int ret = parse_simple_range(left, r->sub_left);
            free(left);
            if (ret != 0) return -1;

            const char* right = and_pos + 2;
            while (isspace((unsigned char)*right)) right++;
            return parse_simple_range(right, r->sub_right);
        }

        /* Try ">=X <Y" pattern (space-separated, no &&) */
        if (len >= 2 && s[0] == '>' && s[1] == '=') {
            /* Find end of first range (next operator) */
            const char* next_op = NULL;
            const char* t = s;
            while (*t) {
                if ((t > s + 1) && (*t == '>' || *t == '<')) {
                    next_op = t;
                    break;
                }
                t++;
            }
            if (next_op) {
                r->type = MG_RANGE_AND;
                r->sub_left = alloc_sub_range();
                r->sub_right = alloc_sub_range();
                if (!r->sub_left || !r->sub_right) return -1;

                size_t left_len = (size_t)(next_op - s);
                char* left = (char*)malloc(left_len + 1);
                if (!left) return -1;
                memcpy(left, s, left_len);
                left[left_len] = '\0';
                int ret = parse_simple_range(left, r->sub_left);
                free(left);
                if (ret != 0) return -1;

                return parse_simple_range(next_op, r->sub_right);
            }
        }
    }

    return parse_simple_range(s, r);
}

static bool simple_range_contains(const mg_range_t* r, const mg_version_t* v) {
    int has_pre = (v->prerelease_len >= 0);

    switch (r->type) {
    case MG_RANGE_STAR:
        return true;

    case MG_RANGE_EXACT:
        if (has_pre) {
            mg_version_t base = *v;
            base.prerelease_len = -1;
            base.prerelease[0] = '\0';
            if (mg_version_cmp(&base, &r->min) != 0) return false;
        }
        return mg_version_cmp(v, &r->min) == 0;

    case MG_RANGE_CARET:
    case MG_RANGE_TILDE:
        if (has_pre) {
            mg_version_t base = *v;
            base.prerelease_len = -1;
            base.prerelease[0] = '\0';
            if (!(mg_version_cmp(&base, &r->min) >= 0 &&
                  mg_version_cmp(&base, &r->max) < 0)) {
                return false;
            }
        }
        return mg_version_cmp(v, &r->min) >= 0 &&
               mg_version_cmp(v, &r->max) < 0;

    case MG_RANGE_GTE:
    case MG_RANGE_GT:
        if (has_pre) {
            if (r->type == MG_RANGE_GTE) {
                if (mg_version_cmp(v, &r->min) < 0) return false;
            } else {
                if (mg_version_cmp(v, &r->min) <= 0) return false;
            }
        }
        if (r->type == MG_RANGE_GTE) return mg_version_cmp(v, &r->min) >= 0;
        return mg_version_cmp(v, &r->min) > 0;

    case MG_RANGE_LTE:
    case MG_RANGE_LT:
        if (r->type == MG_RANGE_LTE) return mg_version_cmp(v, &r->min) <= 0;
        return mg_version_cmp(v, &r->min) < 0;

    default:
        return false;
    }
}

bool mg_range_contains(const mg_range_t* r, const mg_version_t* v) {
    if (!r || !v) return false;

    switch (r->type) {
    case MG_RANGE_OR:
        if (!r->sub_left || !r->sub_right) return false;
        return mg_range_contains(r->sub_left, v) ||
               mg_range_contains(r->sub_right, v);

    case MG_RANGE_AND:
        if (!r->sub_left || !r->sub_right) return false;
        return mg_range_contains(r->sub_left, v) &&
               mg_range_contains(r->sub_right, v);

    default:
        return simple_range_contains(r, v);
    }
}
