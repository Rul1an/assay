#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <stddef.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/syscall.h>
#include <sys/time.h>
#include <sys/un.h>
#include <unistd.h>

#define MUST(c, m) \
    do { \
        if (!(c)) \
            return die(m); \
    } while (0)

static int die(const char *msg) {
    perror(msg);
    return 1;
}

static int set_rcvtimeo(int fd) {
    struct timeval tv = {.tv_sec = 2, .tv_usec = 0};
    return setsockopt(fd, SOL_SOCKET, SO_RCVTIMEO, &tv, sizeof(tv));
}

static int bound_inet(int type, struct sockaddr_in *out) {
    int fd = socket(AF_INET, type, 0);
    struct sockaddr_in addr = {
        .sin_family = AF_INET,
        .sin_addr.s_addr = htonl(INADDR_LOOPBACK),
    };
    socklen_t len = sizeof(addr);
    if (fd < 0) {
        return -1;
    }
    if (bind(fd, (struct sockaddr *)&addr, sizeof(addr)) < 0 ||
        getsockname(fd, (struct sockaddr *)&addr, &len) < 0 ||
        set_rcvtimeo(fd) < 0) {
        close(fd);
        return -1;
    }
    *out = addr;
    return fd;
}

static int recv_byte(int fd, unsigned char want) {
    unsigned char got = 0;
    ssize_t n = recv(fd, &got, 1, 0);
    if (n != 1 || got != want) {
        fprintf(stderr, "recv fail want=0x%02x got=0x%02x n=%zd errno=%d\n", want, got,
                n, errno);
        return -1;
    }
    printf("CELL_OK recv=0x%02x\n", want);
    return 0;
}

static long sys_sendmsg_to(int fd, void *name, socklen_t namelen, unsigned char *b) {
    struct iovec iov = {.iov_base = b, .iov_len = 1};
    struct msghdr msg = {
        .msg_name = name,
        .msg_namelen = namelen,
        .msg_iov = &iov,
        .msg_iovlen = 1,
    };
    return syscall(SYS_sendmsg, fd, &msg, 0);
}

int main(int argc, char **argv) {
    struct sockaddr_in tcp_a, u2, u3, ca, cb;
    struct sockaddr_un un;
    unsigned char b;
    socklen_t ulen;
    int go, tcp_l, tcp_c, acc, r2, s2, r3, s3, fa, fb, ur, us, n;
    char x;

    if (argc == 2 && strcmp(argv[1], "--timeout-selftest") == 0) {
        struct sockaddr_in a;
        int fd = bound_inet(SOCK_DGRAM, &a);
        MUST(fd >= 0 && recv_byte(fd, 0x00) != 0, "timeout selftest");
        printf("TIMEOUT_OK\n");
        return 0;
    }
    if (argc != 2) {
        fprintf(stderr, "usage: %s GO_FIFO\n", argv[0]);
        return 2;
    }
    printf("HARNESS_PID=%d\n", getpid());
    fflush(stdout);
    go = open(argv[1], O_RDONLY);
    MUST(go >= 0 && read(go, &x, 1) >= 1, "GO fifo");
    close(go);

    tcp_l = bound_inet(SOCK_STREAM, &tcp_a);
    MUST(tcp_l >= 0 && listen(tcp_l, 1) == 0, "tcp listen");
    printf("CELL1_TCP_PORT=%u\n", ntohs(tcp_a.sin_port));
    fflush(stdout);
    tcp_c = socket(AF_INET, SOCK_STREAM, 0);
    MUST(tcp_c >= 0 && syscall(SYS_connect, tcp_c, &tcp_a, sizeof(tcp_a)) == 0,
         "SYS_connect");
    acc = accept(tcp_l, NULL, NULL);
    MUST(acc >= 0, "accept");
    printf("CELL_OK 1 accept\n");
    close(acc);
    close(tcp_c);
    close(tcp_l);

    r2 = bound_inet(SOCK_DGRAM, &u2);
    s2 = socket(AF_INET, SOCK_DGRAM, 0);
    MUST(r2 >= 0 && s2 >= 0, "cell2 sockets");
    printf("CELL2_UDP_PORT=%u\n", ntohs(u2.sin_port));
    fflush(stdout);
    b = 0xA2;
    MUST(syscall(SYS_sendto, s2, &b, 1, 0, &u2, sizeof(u2)) == 1 &&
             recv_byte(r2, 0xA2) == 0,
         "cell2 sendto");
    close(s2);
    close(r2);

    r3 = bound_inet(SOCK_DGRAM, &u3);
    s3 = socket(AF_INET, SOCK_DGRAM, 0);
    MUST(r3 >= 0 && s3 >= 0, "cell3 sockets");
    printf("CELL3_UDP_PORT=%u\n", ntohs(u3.sin_port));
    fflush(stdout);
    b = 0xA3;
    MUST(sys_sendmsg_to(s3, &u3, sizeof(u3), &b) == 1 && recv_byte(r3, 0xA3) == 0,
         "cell3 sendmsg");
    close(s3);
    close(r3);

    fa = bound_inet(SOCK_DGRAM, &ca);
    fb = bound_inet(SOCK_DGRAM, &cb);
    MUST(fa >= 0 && fb >= 0 &&
             syscall(SYS_connect, fa, &cb, sizeof(cb)) == 0 &&
             syscall(SYS_connect, fb, &ca, sizeof(ca)) == 0,
         "udp pair connect");
    b = 0xA4;
    MUST(syscall(SYS_sendto, fa, &b, 1, 0, NULL, 0) == 1 && recv_byte(fb, 0xA4) == 0,
         "cell4 sendto");
    b = 0xA5;
    MUST(sys_sendmsg_to(fa, NULL, 0, &b) == 1 && recv_byte(fb, 0xA5) == 0,
         "cell4 sendmsg");
    close(fa);
    close(fb);

    memset(&un, 0, sizeof(un));
    un.sun_family = AF_UNIX;
    n = snprintf(un.sun_path + 1, sizeof(un.sun_path) - 1, "assay-s1b-%d", getpid());
    ulen = (socklen_t)(offsetof(struct sockaddr_un, sun_path) + 1 + (size_t)n);
    ur = socket(AF_UNIX, SOCK_DGRAM, 0);
    us = socket(AF_UNIX, SOCK_DGRAM, 0);
    MUST(ur >= 0 && us >= 0 && bind(ur, (struct sockaddr *)&un, ulen) == 0 &&
             set_rcvtimeo(ur) == 0,
         "unix bind");
    b = 0xA6;
    MUST(syscall(SYS_sendto, us, &b, 1, 0, &un, ulen) == 1 && recv_byte(ur, 0xA6) == 0,
         "cell5 sendto");
    b = 0xA7;
    MUST(sys_sendmsg_to(us, &un, ulen, &b) == 1 && recv_byte(ur, 0xA7) == 0,
         "cell5 sendmsg");
    close(us);
    close(ur);
    printf("HARNESS_OK\n");
    return 0;
}
