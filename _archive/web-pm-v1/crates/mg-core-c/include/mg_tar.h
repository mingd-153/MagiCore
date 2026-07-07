#ifndef MG_TAR_H
#define MG_TAR_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define MG_TAR_MAX_PATH 512
#define MG_SHA256_HEX_SIZE 65

/* Tar entry type constants */
#define MG_TAR_TYPE_FILE     '0'
#define MG_TAR_TYPE_FILE_ALT '\0'
#define MG_TAR_TYPE_HARDLINK '1'
#define MG_TAR_TYPE_SYMLINK  '2'
#define MG_TAR_TYPE_DIR      '5'

/* Result for one extracted tar entry.
 * path, sha256_hex point to internal buffers valid until next callback call.
 * data is owned by the caller after the callback returns.
 */
typedef struct {
    char path[MG_TAR_MAX_PATH];
    unsigned char* data;
    size_t data_len;
    int is_executable;
    char sha256_hex[MG_SHA256_HEX_SIZE];
} mg_tar_entry_t;

/* Callback for each file entry.
 * Returns 0 to continue, non-zero to abort extraction.
 * If cb returns non-zero, the data pointer is freed automatically.
 * If cb returns 0, the caller takes ownership of entry.data (must free later).
 */
typedef int (*mg_tar_entry_cb)(mg_tar_entry_t* entry, void* userdata);

/* Extract a gzip-compressed tar archive in one pass.
 * For each regular file entry, the callback is invoked with decompressed data
 * and its SHA-256 hex hash. Directories and symlinks are skipped.
 *
 * Returns 0 on success, -1 on error.
 */
int mg_tar_extract(const unsigned char* gz_data, size_t gz_len,
                   mg_tar_entry_cb callback, void* userdata);

/* Free entry data allocated by mg_tar_extract */
void mg_tar_entry_free(mg_tar_entry_t* entry);

#ifdef __cplusplus
}
#endif

#endif /* MG_TAR_H */
