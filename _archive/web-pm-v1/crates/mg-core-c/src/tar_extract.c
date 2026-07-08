#include "mg_tar.h"
#include "mg_sha256.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <zlib.h>

#pragma pack(push, 1)
typedef struct {
    char name[100];
    char mode[8];
    char uid[8];
    char gid[8];
    char size[12];
    char mtime[12];
    char chksum[8];
    char typeflag;
    char linkname[100];
    char magic[6];
    char version[2];
    char uname[32];
    char gname[32];
    char devmajor[8];
    char devminor[8];
    char prefix[155];
    char padding[12];
} tar_hdr_t;
#pragma pack(pop)

static uint64_t parse_octal(const char* s, size_t len) {
    uint64_t v = 0;
    for (size_t i = 0; i < len; i++)
        if (s[i] >= '0' && s[i] <= '7')
            v = (v << 3) | (uint64_t)(s[i] - '0');
    return v;
}

static int is_zero_block(const unsigned char* b) {
    for (int i = 0; i < 512; i++)
        if (b[i]) return 0;
    return 1;
}

static void build_path(char* out, size_t n, const tar_hdr_t* h) {
    int hp = 0;
    for (int i = 0; i < 155; i++)
        if (h->prefix[i] && h->prefix[i] != ' ') { hp = 1; break; }
    if (hp)
        snprintf(out, n, "%.*s/%.*s", 155, h->prefix, 100, h->name);
    else
        snprintf(out, n, "%.*s", 100, h->name);
    size_t sl = strlen(out);
    while (sl > 0 && out[sl-1] == '/') out[--sl] = '\0';
}

static const char* strip_pkg(const char* name) {
    if (!name) return "";
    if (strncmp(name, "package/", 8) == 0) return name + 8;
    if (strcmp(name, "package") == 0) return ".";
    return name;
}

void mg_tar_entry_free(mg_tar_entry_t* e) {
    if (e && e->data) { free(e->data); e->data = NULL; e->data_len = 0; }
}

int mg_tar_extract(const unsigned char* gz, size_t gz_len,
                   mg_tar_entry_cb cb, void* ud) {
    if (!gz || !cb) return -1;

    z_stream strm;
    memset(&strm, 0, sizeof(strm));
    if (inflateInit2(&strm, 16 + MAX_WBITS) != Z_OK) return -1;

    strm.next_in = (unsigned char*)gz;
    strm.avail_in = (unsigned int)gz_len;

    size_t cap = 65536;
    size_t len = 0;
    unsigned char* raw = (unsigned char*)malloc(cap);
    if (!raw) { inflateEnd(&strm); return -1; }

    int ret;
    do {
        if (len + 4096 > cap) {
            cap = cap * 2 + 65536;
            unsigned char* tmp = (unsigned char*)realloc(raw, cap);
            if (!tmp) { free(raw); inflateEnd(&strm); return -1; }
            raw = tmp;
        }
        strm.next_out = raw + len;
        strm.avail_out = (unsigned int)(cap - len);
        ret = inflate(&strm, Z_NO_FLUSH);
        if (ret == Z_OK || ret == Z_STREAM_END)
            len = cap - strm.avail_out;
    } while (ret == Z_OK);

    if (ret != Z_STREAM_END && ret != Z_OK) {
        free(raw); inflateEnd(&strm); return -1;
    }

    size_t pos = 0;
    int zero_cnt = 0;

    while (pos + 512 <= len) {
        if (is_zero_block(raw + pos)) {
            zero_cnt++;
            pos += 512;
            if (zero_cnt >= 2) break;
            continue;
        }
        zero_cnt = 0;

        tar_hdr_t* h = (tar_hdr_t*)(raw + pos);
        pos += 512;

        uint64_t fsize = 0;
        if (h->size[0]) {
            char sb[13]; memcpy(sb, h->size, 12); sb[12] = '\0';
            fsize = parse_octal(sb, 12);
        }
        uint64_t padded = ((fsize + 511) / 512) * 512;

        char nb[MG_TAR_MAX_PATH];
        build_path(nb, sizeof(nb), h);
        const char* sp = strip_pkg(nb);
        (void)sp;

        uint64_t mode_v = parse_octal(h->mode, 8);
        int is_exec = (mode_v & 0111) != 0;

        if (h->typeflag == MG_TAR_TYPE_FILE || h->typeflag == MG_TAR_TYPE_FILE_ALT) {
            mg_tar_entry_t entry;
            memset(&entry, 0, sizeof(entry));
            strncpy(entry.path, sp, MG_TAR_MAX_PATH - 1);
            entry.path[MG_TAR_MAX_PATH - 1] = '\0';
            entry.is_executable = is_exec;

            if (fsize > 0) {
                if (pos + (size_t)fsize > len) { pos += (size_t)padded; continue; }
                entry.data = (unsigned char*)malloc((size_t)fsize);
                if (!entry.data) { free(raw); inflateEnd(&strm); return -1; }
                memcpy(entry.data, raw + pos, (size_t)fsize);
                entry.data_len = (size_t)fsize;
                mg_sha256_hash(entry.data, (size_t)fsize, entry.sha256_hex);
            } else {
                entry.data = (unsigned char*)malloc(1);
                if (entry.data) entry.data[0] = 0;
                entry.data_len = 0;
                mg_sha256_hash("", 0, entry.sha256_hex);
            }

            int r = cb(&entry, ud);
            if (entry.data) free(entry.data);
            entry.data = NULL;
            if (r != 0) { free(raw); inflateEnd(&strm); return r; }
        }

        pos += (size_t)padded;
    }

    free(raw);
    inflateEnd(&strm);
    return 0;
}
