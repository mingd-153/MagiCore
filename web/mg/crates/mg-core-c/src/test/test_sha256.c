#include "mg_sha256.h"
#include <stdio.h>
#include <string.h>
#include <assert.h>

void test_empty_string() {
    char hex[65];
    mg_sha256_hash("", 0, hex);
    assert(strcmp(hex, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855") == 0);
    printf("  PASS test_empty_string\n");
}

void test_hello() {
    char hex[65];
    mg_sha256_hash("hello", 5, hex);
    assert(strcmp(hex, "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824") == 0);
    printf("  PASS test_hello\n");
}

void test_streaming() {
    mg_sha256_ctx_t ctx;
    mg_sha256_init(&ctx);
    mg_sha256_update(&ctx, "hello", 5);
    mg_sha256_update(&ctx, " ", 1);
    mg_sha256_update(&ctx, "world", 5);

    char hex[65];
    mg_sha256_final_hex(&ctx, hex);
    assert(strcmp(hex, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9") == 0);
    printf("  PASS test_streaming\n");
}

void test_nist_vector() {
    /* NIST SHA-256 test vector: "abc" */
    char hex[65];
    mg_sha256_hash("abc", 3, hex);
    assert(strcmp(hex, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad") == 0);
    printf("  PASS test_nist_vector\n");
}

void test_large_input() {
    /* Hash 10,000 'a' characters */
    char buf[10000];
    memset(buf, 'a', sizeof(buf));
    char hex[65];
    mg_sha256_hash(buf, sizeof(buf), hex);
    assert(strcmp(hex, "f2c688be7ced5aafdc3eb2b2b44d89b20753b291f057a5c4cb9d5fb6b9c314f9") == 0);
    printf("  PASS test_large_input\n");
}

int main() {
    printf("SHA-256 tests:\n");
    test_empty_string();
    test_hello();
    test_streaming();
    test_nist_vector();
    test_large_input();
    printf("All SHA-256 tests PASSED\n");
    return 0;
}
