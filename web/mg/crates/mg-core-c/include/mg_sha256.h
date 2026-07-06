#ifndef MG_SHA256_H
#define MG_SHA256_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define MG_SHA256_HEX_SIZE 65  /* 64 hex chars + null */

/* Opaque context for streaming SHA-256 (256 bits = 32 bytes internal). */
typedef struct {
    uint64_t count;
    uint32_t state[8];
    uint8_t buffer[64];
} mg_sha256_ctx_t;

/* Initialize SHA-256 context. */
void mg_sha256_init(mg_sha256_ctx_t* ctx);

/* Feed data into hash. */
void mg_sha256_update(mg_sha256_ctx_t* ctx, const void* data, size_t len);

/* Finalise and write hex digest to out (must be MG_SHA256_HEX_SIZE bytes). */
void mg_sha256_final_hex(mg_sha256_ctx_t* ctx, char* out);

/* Finalise and write raw 32-byte digest to out. */
void mg_sha256_final_raw(mg_sha256_ctx_t* ctx, uint8_t* out);

/* One-shot: hash a buffer and write hex digest. */
void mg_sha256_hash(const void* data, size_t len, char* out);

#ifdef __cplusplus
}
#endif

#endif /* MG_SHA256_H */
