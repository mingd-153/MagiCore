#include "mg_sha256.h"
#include <string.h>

/* ── SHA-256 implementation (FIPS 180-4) ── */

static const uint32_t K[64] = {
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
    0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
    0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
    0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
    0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
};

#define ROTR(x, n) (((x) >> (n)) | ((x) << (32 - (n))))
#define SHR(x, n)  ((x) >> (n))

#define CH(x, y, z)  (((x) & (y)) ^ (~(x) & (z)))
#define MAJ(x, y, z) (((x) & (y)) ^ ((x) & (z)) ^ ((y) & (z)))

#define EP0(x) (ROTR(x, 2) ^ ROTR(x, 13) ^ ROTR(x, 22))
#define EP1(x) (ROTR(x, 6) ^ ROTR(x, 11) ^ ROTR(x, 25))
#define SIG0(x) (ROTR(x, 7) ^ ROTR(x, 18) ^ SHR(x, 3))
#define SIG1(x) (ROTR(x, 17) ^ ROTR(x, 19) ^ SHR(x, 10))

static void sha256_transform(mg_sha256_ctx_t* ctx, const uint8_t block[64]) {
    uint32_t W[64];
    for (int t = 0; t < 16; t++) {
        W[t] = ((uint32_t)block[t * 4] << 24) |
               ((uint32_t)block[t * 4 + 1] << 16) |
               ((uint32_t)block[t * 4 + 2] << 8) |
               ((uint32_t)block[t * 4 + 3]);
    }
    for (int t = 16; t < 64; t++) {
        W[t] = SIG1(W[t-2]) + W[t-7] + SIG0(W[t-15]) + W[t-16];
    }

    uint32_t a = ctx->state[0];
    uint32_t b = ctx->state[1];
    uint32_t c = ctx->state[2];
    uint32_t d = ctx->state[3];
    uint32_t e = ctx->state[4];
    uint32_t f = ctx->state[5];
    uint32_t g = ctx->state[6];
    uint32_t h = ctx->state[7];

    for (int t = 0; t < 64; t++) {
        uint32_t T1 = h + EP1(e) + CH(e, f, g) + K[t] + W[t];
        uint32_t T2 = EP0(a) + MAJ(a, b, c);
        h = g;
        g = f;
        f = e;
        e = d + T1;
        d = c;
        c = b;
        b = a;
        a = T1 + T2;
    }

    ctx->state[0] += a;
    ctx->state[1] += b;
    ctx->state[2] += c;
    ctx->state[3] += d;
    ctx->state[4] += e;
    ctx->state[5] += f;
    ctx->state[6] += g;
    ctx->state[7] += h;
}

void mg_sha256_init(mg_sha256_ctx_t* ctx) {
    ctx->count = 0;
    ctx->state[0] = 0x6a09e667;
    ctx->state[1] = 0xbb67ae85;
    ctx->state[2] = 0x3c6ef372;
    ctx->state[3] = 0xa54ff53a;
    ctx->state[4] = 0x510e527f;
    ctx->state[5] = 0x9b05688c;
    ctx->state[6] = 0x1f83d9ab;
    ctx->state[7] = 0x5be0cd19;
    memset(ctx->buffer, 0, sizeof(ctx->buffer));
}

void mg_sha256_update(mg_sha256_ctx_t* ctx, const void* data, size_t len) {
    const uint8_t* bytes = (const uint8_t*)data;
    size_t idx = (size_t)(ctx->count & 0x3f);
    ctx->count += (uint64_t)len;

    if (idx > 0) {
        size_t fill = 64 - idx;
        if (len < fill) {
            memcpy(ctx->buffer + idx, bytes, len);
            return;
        }
        memcpy(ctx->buffer + idx, bytes, fill);
        sha256_transform(ctx, ctx->buffer);
        bytes += fill;
        len -= fill;
    }

    while (len >= 64) {
        sha256_transform(ctx, bytes);
        bytes += 64;
        len -= 64;
    }

    if (len > 0) {
        memcpy(ctx->buffer, bytes, len);
    }
}

static void sha256_finalize(mg_sha256_ctx_t* ctx) {
    uint64_t bits = ctx->count * 8;
    size_t idx = (size_t)(ctx->count & 0x3f);
    size_t pad_len = (idx < 56) ? (56 - idx) : (120 - idx);

    uint8_t padding[128];
    padding[0] = 0x80;
    memset(padding + 1, 0, pad_len - 1);

    mg_sha256_update(ctx, padding, pad_len);

    uint8_t len_bytes[8];
    for (int i = 0; i < 8; i++) {
        len_bytes[i] = (uint8_t)(bits >> (56 - i * 8));
    }
    mg_sha256_update(ctx, len_bytes, 8);
}

void mg_sha256_final_hex(mg_sha256_ctx_t* ctx, char* out) {
    sha256_finalize(ctx);

    for (int i = 0; i < 8; i++) {
        int n = 0;
        for (int b = 0; b < 4; b++) {
            uint8_t byte = (uint8_t)(ctx->state[i] >> (24 - b * 8));
            const char* hex = "0123456789abcdef";
            out[n++] = hex[byte >> 4];
            out[n++] = hex[byte & 0x0f];
        }
        out += 8;
    }
    *out = '\0';
}

void mg_sha256_final_raw(mg_sha256_ctx_t* ctx, uint8_t* out) {
    sha256_finalize(ctx);

    for (int i = 0; i < 8; i++) {
        for (int b = 0; b < 4; b++) {
            *out++ = (uint8_t)(ctx->state[i] >> (24 - b * 8));
        }
    }
}

void mg_sha256_hash(const void* data, size_t len, char* out) {
    mg_sha256_ctx_t ctx;
    mg_sha256_init(&ctx);
    mg_sha256_update(&ctx, data, len);
    mg_sha256_final_hex(&ctx, out);
}
