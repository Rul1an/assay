/* Inert local fixture for the bounded OCI executor contract.
 *
 * Built into a FROM scratch image in tests. Not a candidate implementation.
 * Modes are argv[1]: ok (default), sleep, flood, oom, read.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int main(int argc, char **argv)
{
    const char *mode = argc > 1 ? argv[1] : "ok";

    if (strcmp(mode, "ok") == 0) {
        return 0;
    }
    if (strcmp(mode, "sleep") == 0) {
        for (;;) {
            sleep(60);
        }
    }
    if (strcmp(mode, "flood") == 0) {
        static const char chunk[4096];
        for (;;) {
            if (fwrite(chunk, 1, sizeof(chunk), stdout) != sizeof(chunk)) {
                return 1;
            }
        }
    }
    if (strcmp(mode, "oom") == 0) {
        for (;;) {
            volatile char *block = (volatile char *)malloc(1024 * 1024);
            if (block != NULL) {
                block[0] = 1;
                block[1024 * 1024 - 1] = 1;
            }
        }
    }
    if (strcmp(mode, "read") == 0) {
        FILE *fp = fopen("/input/bundle.tar.gz", "rb");
        char buf[4096];
        size_t n;

        if (fp == NULL) {
            return 1;
        }
        while ((n = fread(buf, 1, sizeof(buf), fp)) > 0) {
            if (fwrite(buf, 1, n, stdout) != n) {
                fclose(fp);
                return 1;
            }
        }
        if (ferror(fp)) {
            fclose(fp);
            return 1;
        }
        fclose(fp);
        return 0;
    }
    return 2;
}
