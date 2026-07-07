#include "mg_semver.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int tests_passed = 0;
static int tests_failed = 0;

#define TEST(name) do { printf("  TEST: %s ... ", name); } while(0)
#define PASS() do { printf("PASS\n"); tests_passed++; } while(0)
#define FAIL(msg) do { printf("FAIL: %s\n", msg); tests_failed++; } while(0)
#define ASSERT(cond) do { if (!(cond)) { FAIL(#cond); return; } } while(0)

static void test_version_parse_basic(void) {
    TEST("parse basic 1.2.3");
    mg_version_t v;
    ASSERT(mg_version_parse("1.2.3", &v) == 0);
    ASSERT(v.major == 1);
    ASSERT(v.minor == 2);
    ASSERT(v.patch == 3);
    ASSERT(v.prerelease_len == -1);
    PASS();
}

static void test_version_parse_prerelease(void) {
    TEST("parse 1.0.0-alpha.1");
    mg_version_t v;
    ASSERT(mg_version_parse("1.0.0-alpha.1", &v) == 0);
    ASSERT(v.major == 1);
    ASSERT(v.minor == 0);
    ASSERT(v.patch == 0);
    ASSERT(v.prerelease_len > 0);
    ASSERT(strcmp(v.prerelease, "alpha.1") == 0);
    PASS();
}

static void test_version_parse_build(void) {
    TEST("parse 2.0.0+build.123");
    mg_version_t v;
    ASSERT(mg_version_parse("2.0.0+build.123", &v) == 0);
    ASSERT(v.major == 2);
    ASSERT(v.prerelease_len == -1);
    PASS();
}

static void test_version_parse_prerelease_build(void) {
    TEST("parse 1.0.0-rc.1+build.5");
    mg_version_t v;
    ASSERT(mg_version_parse("1.0.0-rc.1+build.5", &v) == 0);
    ASSERT(strcmp(v.prerelease, "rc.1") == 0);
    PASS();
}

static void test_version_parse_invalid(void) {
    TEST("parse invalid");
    mg_version_t v;
    ASSERT(mg_version_parse("", &v) != 0);
    ASSERT(mg_version_parse("abc", &v) != 0);
    ASSERT(mg_version_parse("1.2", &v) != 0);
    ASSERT(mg_version_parse("1.2.3.4", &v) != 0);
    PASS();
}

static void test_version_cmp_major(void) {
    TEST("cmp major");
    mg_version_t a, b;
    mg_version_parse("2.0.0", &a);
    mg_version_parse("1.0.0", &b);
    ASSERT(mg_version_cmp(&a, &b) == 1);
    ASSERT(mg_version_cmp(&b, &a) == -1);
    PASS();
}

static void test_version_cmp_prerelease_numeric(void) {
    TEST("cmp prerelease numeric: next.9 vs next.24");
    mg_version_t a, b;
    mg_version_parse("1.0.0-next.9", &a);
    mg_version_parse("1.0.0-next.24", &b);
    ASSERT(mg_version_cmp(&a, &b) == -1);  /* 9 < 24 */
    ASSERT(mg_version_cmp(&b, &a) == 1);   /* 24 > 9 */
    PASS();
}

static void test_version_cmp_prerelease_mixed(void) {
    TEST("cmp prerelease mixed: numeric < string");
    mg_version_t a, b;
    mg_version_parse("1.0.0-1", &a);
    mg_version_parse("1.0.0-alpha", &b);
    ASSERT(mg_version_cmp(&a, &b) == -1);  /* numeric < string */
    ASSERT(mg_version_cmp(&b, &a) == 1);
    PASS();
}

static void test_version_cmp_release_vs_prerelease(void) {
    TEST("cmp release vs prerelease");
    mg_version_t a, b;
    mg_version_parse("1.0.0", &a);
    mg_version_parse("1.0.0-alpha", &b);
    ASSERT(mg_version_cmp(&a, &b) == 1);   /* release > prerelease */
    ASSERT(mg_version_cmp(&b, &a) == -1);
    PASS();
}

static void test_version_cmp_equal(void) {
    TEST("cmp equal");
    mg_version_t a, b;
    mg_version_parse("1.0.0", &a);
    mg_version_parse("1.0.0", &b);
    ASSERT(mg_version_cmp(&a, &b) == 0);
    PASS();
}

static void test_version_cmp_prerelease_shorter(void) {
    TEST("cmp prerelease: fewer fields < more fields");
    mg_version_t a, b;
    mg_version_parse("1.0.0-alpha", &a);
    mg_version_parse("1.0.0-alpha.1", &b);
    ASSERT(mg_version_cmp(&a, &b) == -1);  /* alpha < alpha.1 */
    PASS();
}

static void test_range_parse_caret(void) {
    TEST("range parse ^1.0.0");
    mg_range_t r;
    ASSERT(mg_range_parse("^1.0.0", &r) == 0);
    ASSERT(r.type == MG_RANGE_CARET);
    ASSERT(r.min.major == 1);
    ASSERT(r.max.major == 2);
    PASS();
}

static void test_range_parse_tilde(void) {
    TEST("range parse ~1.2.0");
    mg_range_t r;
    ASSERT(mg_range_parse("~1.2.0", &r) == 0);
    ASSERT(r.type == MG_RANGE_TILDE);
    ASSERT(r.min.major == 1);
    ASSERT(r.min.minor == 2);
    ASSERT(r.max.minor == 3);
    PASS();
}

static void test_range_parse_star(void) {
    TEST("range parse *");
    mg_range_t r;
    ASSERT(mg_range_parse("*", &r) == 0);
    ASSERT(r.type == MG_RANGE_STAR);
    PASS();
}

static void test_range_parse_gte(void) {
    TEST("range parse >=1.0.0");
    mg_range_t r;
    ASSERT(mg_range_parse(">=1.0.0", &r) == 0);
    ASSERT(r.type == MG_RANGE_GTE);
    PASS();
}

static void test_range_parse_and(void) {
    TEST("range parse >=1.0.0 <2.0.0");
    mg_range_t r;
    ASSERT(mg_range_parse(">=1.0.0 <2.0.0", &r) == 0);
    ASSERT(r.type == MG_RANGE_AND);
    PASS();
}

static void test_range_parse_or(void) {
    TEST("range parse ^1.0.0 || ^2.0.0");
    mg_range_t r;
    ASSERT(mg_range_parse("^1.0.0 || ^2.0.0", &r) == 0);
    ASSERT(r.type == MG_RANGE_OR);
    PASS();
}

static void test_range_contains_caret(void) {
    TEST("range contains ^1.0.0");
    mg_range_t r;
    mg_version_t v;
    mg_range_parse("^1.0.0", &r);

    mg_version_parse("1.0.0", &v);
    ASSERT(mg_range_contains(&r, &v));

    mg_version_parse("1.5.0", &v);
    ASSERT(mg_range_contains(&r, &v));

    mg_version_parse("2.0.0", &v);
    ASSERT(!mg_range_contains(&r, &v));  /* 2.0.0 outside ^1.0.0 */

    mg_version_parse("0.9.0", &v);
    ASSERT(!mg_range_contains(&r, &v));
    PASS();
}

static void test_range_contains_caret_prerelease(void) {
    TEST("range contains ^1.0.0-next.24");
    mg_range_t r;
    mg_version_t v;
    mg_range_parse("^1.0.0-next.24", &r);

    /* next.9 should NOT match ^1.0.0-next.24 (numeric comparison) */
    mg_version_parse("1.0.0-next.9", &v);
    ASSERT(!mg_range_contains(&r, &v));

    /* next.24 should match */
    mg_version_parse("1.0.0-next.24", &v);
    ASSERT(mg_range_contains(&r, &v));

    /* next.29 should match */
    mg_version_parse("1.0.0-next.29", &v);
    ASSERT(mg_range_contains(&r, &v));

    /* 2.0.0-next.1 should NOT match (major outside range) */
    mg_version_parse("2.0.0-next.1", &v);
    ASSERT(!mg_range_contains(&r, &v));
    PASS();
}

static void test_range_contains_exact(void) {
    TEST("range contains 1.2.3");
    mg_range_t r;
    mg_version_t v;
    mg_range_parse("1.2.3", &r);

    mg_version_parse("1.2.3", &v);
    ASSERT(mg_range_contains(&r, &v));

    mg_version_parse("1.2.4", &v);
    ASSERT(!mg_range_contains(&r, &v));
    PASS();
}

static void test_range_contains_tilde(void) {
    TEST("range contains ~1.2.0");
    mg_range_t r;
    mg_version_t v;
    mg_range_parse("~1.2.0", &r);

    mg_version_parse("1.2.0", &v);
    ASSERT(mg_range_contains(&r, &v));

    mg_version_parse("1.2.9", &v);
    ASSERT(mg_range_contains(&r, &v));

    mg_version_parse("1.3.0", &v);
    ASSERT(!mg_range_contains(&r, &v));
    PASS();
}

static void test_range_contains_star(void) {
    TEST("range contains *");
    mg_range_t r;
    mg_version_t v;
    mg_range_parse("*", &r);

    mg_version_parse("1.0.0", &v);
    ASSERT(mg_range_contains(&r, &v));

    mg_version_parse("100.200.300", &v);
    ASSERT(mg_range_contains(&r, &v));
    PASS();
}

static void test_range_contains_gte(void) {
    TEST("range contains >=1.0.0");
    mg_range_t r;
    mg_version_t v;
    mg_range_parse(">=1.0.0", &r);

    mg_version_parse("1.0.0", &v);
    ASSERT(mg_range_contains(&r, &v));

    mg_version_parse("5.0.0", &v);
    ASSERT(mg_range_contains(&r, &v));

    mg_version_parse("0.9.0", &v);
    ASSERT(!mg_range_contains(&r, &v));
    PASS();
}

static void test_range_contains_gt(void) {
    TEST("range contains >1.0.0");
    mg_range_t r;
    mg_version_t v;
    mg_range_parse(">1.0.0", &r);

    mg_version_parse("1.0.0", &v);
    ASSERT(!mg_range_contains(&r, &v));

    mg_version_parse("1.0.1", &v);
    ASSERT(mg_range_contains(&r, &v));
    PASS();
}

static void test_range_contains_and(void) {
    TEST("range contains >=1.0.0 <2.0.0");
    mg_range_t r;
    mg_version_t v;
    mg_range_parse(">=1.0.0 <2.0.0", &r);

    mg_version_parse("1.5.0", &v);
    ASSERT(mg_range_contains(&r, &v));

    mg_version_parse("2.0.0", &v);
    ASSERT(!mg_range_contains(&r, &v));

    mg_version_parse("0.9.0", &v);
    ASSERT(!mg_range_contains(&r, &v));
    PASS();
}

static void test_range_contains_or(void) {
    TEST("range contains ^1.0.0 || ^2.0.0");
    mg_range_t r;
    mg_version_t v;
    mg_range_parse("^1.0.0 || ^2.0.0", &r);

    mg_version_parse("1.5.0", &v);
    ASSERT(mg_range_contains(&r, &v));

    mg_version_parse("2.3.0", &v);
    ASSERT(mg_range_contains(&r, &v));

    mg_version_parse("3.0.0", &v);
    ASSERT(!mg_range_contains(&r, &v));
    PASS();
}

/* Regression: @polka/url next.9 < next.24 */
static void test_polka_url_regression(void) {
    TEST("@polka/url regression: next.9 should NOT satisfy ^1.0.0-next.24");
    mg_range_t r;
    mg_version_t v9, v24, v29;
    mg_range_parse("^1.0.0-next.24", &r);
    mg_version_parse("1.0.0-next.9", &v9);
    mg_version_parse("1.0.0-next.24", &v24);
    mg_version_parse("1.0.0-next.29", &v29);

    ASSERT(!mg_range_contains(&r, &v9));
    ASSERT(mg_range_contains(&r, &v24));
    ASSERT(mg_range_contains(&r, &v29));
    PASS();
}

int main(void) {
    printf("mg_semver C tests\n");
    printf("=================\n\n");

    test_version_parse_basic();
    test_version_parse_prerelease();
    test_version_parse_build();
    test_version_parse_prerelease_build();
    test_version_parse_invalid();
    test_version_cmp_major();
    test_version_cmp_prerelease_numeric();
    test_version_cmp_prerelease_mixed();
    test_version_cmp_release_vs_prerelease();
    test_version_cmp_equal();
    test_version_cmp_prerelease_shorter();
    test_range_parse_caret();
    test_range_parse_tilde();
    test_range_parse_star();
    test_range_parse_gte();
    test_range_parse_and();
    test_range_parse_or();
    test_range_contains_caret();
    test_range_contains_caret_prerelease();
    test_range_contains_exact();
    test_range_contains_tilde();
    test_range_contains_star();
    test_range_contains_gte();
    test_range_contains_gt();
    test_range_contains_and();
    test_range_contains_or();
    test_polka_url_regression();

    printf("\n=================\n");
    printf("Results: %d passed, %d failed\n", tests_passed, tests_failed);
    return tests_failed > 0 ? 1 : 0;
}
