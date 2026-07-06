#include "mg_json.h"
#include <stdio.h>
#include <string.h>
#include <assert.h>

static const char* SAMPLE = "{"
    "\"name\": \"react\","
    "\"version\": \"18.2.0\","
    "\"dist-tags\": { \"latest\": \"18.2.0\" },"
    "\"versions\": {"
        "\"18.2.0\": {"
            "\"name\": \"react\","
            "\"version\": \"18.2.0\","
            "\"dependencies\": { \"loose-envify\": \"^1.1.0\" },"
            "\"optionalDependencies\": {}"
        "},"
        "\"19.0.0\": {"
            "\"name\": \"react\","
            "\"version\": \"19.0.0\","
            "\"dependencies\": {}"
        "}"
    "}"
"}";

static int count_cb(
    const char* key, size_t key_len,
    const char* val, size_t val_len,
    void* ctx
) {
    (void)key; (void)key_len; (void)val; (void)val_len;
    int* count = (int*)ctx;
    (*count)++;
    return 0;
}

void test_get_string() {
    char buf[256];
    int r = mg_json_get_string(SAMPLE, "name", buf, sizeof(buf));
    assert(r == 0);
    assert(strcmp(buf, "react") == 0);

    r = mg_json_get_string(SAMPLE, "version", buf, sizeof(buf));
    assert(r == 0);
    assert(strcmp(buf, "18.2.0") == 0);

    printf("  PASS test_get_string\n");
}

void test_get_string_missing() {
    char buf[256];
    int r = mg_json_get_string("{\"a\": 1}", "b", buf, sizeof(buf));
    assert(r == -1);
    printf("  PASS test_get_string_missing\n");
}

void test_get_string_not_string() {
    char buf[256];
    int r = mg_json_get_string("{\"a\": 42}", "a", buf, sizeof(buf));
    assert(r == -1);
    printf("  PASS test_get_string_not_string\n");
}

void test_get_int() {
    int val;
    int r = mg_json_get_int("{\"count\": 42}", "count", &val);
    assert(r == 0);
    assert(val == 42);
    printf("  PASS test_get_int\n");
}

void test_get_int_missing() {
    int val;
    int r = mg_json_get_int("{\"a\": 1}", "b", &val);
    assert(r == -1);
    printf("  PASS test_get_int_missing\n");
}

void test_iterate_versions() {
    int count = 0;
    int r = mg_json_iterate_versions(SAMPLE, count_cb, &count);
    assert(r == 0);
    assert(count == 2);
    printf("  PASS test_iterate_versions (%d)\n", count);
}

void test_iterate_deps() {
    int count = 0;
    int r = mg_json_iterate_deps(SAMPLE, "18.2.0", count_cb, &count);
    assert(r == 0);
    assert(count == 1);
    printf("  PASS test_iterate_deps (%d)\n", count);
}

void test_iterate_deps_empty() {
    int count = 0;
    int r = mg_json_iterate_deps(SAMPLE, "19.0.0", count_cb, &count);
    assert(r == 0);
    assert(count == 0);
    printf("  PASS test_iterate_deps_empty\n");
}

void test_dotted_path() {
    char buf[256];
    int r = mg_json_get_string(SAMPLE, "dist-tags.latest", buf, sizeof(buf));
    assert(r == 0);
    assert(strcmp(buf, "18.2.0") == 0);
    printf("  PASS test_dotted_path\n");
}

void test_empty_object() {
    int count = 0;
    int r = mg_json_object_for_each("{}", count_cb, &count);
    assert(r == 0);
    assert(count == 0);
    printf("  PASS test_empty_object\n");
}

void test_invalid_json() {
    char buf[256];
    int r = mg_json_get_string("not json", "key", buf, sizeof(buf));
    assert(r == -1);
    printf("  PASS test_invalid_json\n");
}

int main() {
    printf("JSON tests:\n");
    test_get_string();
    test_get_string_missing();
    test_get_string_not_string();
    test_get_int();
    test_get_int_missing();
    test_iterate_versions();
    test_iterate_deps();
    test_iterate_deps_empty();
    test_dotted_path();
    test_empty_object();
    test_invalid_json();
    printf("All JSON tests PASSED\n");
    return 0;
}
