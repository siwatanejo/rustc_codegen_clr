/// A list of all functions which are redirected to system libc.
pub const LIBC_FNS: [&str; 504] = [
    "a64l",
    "abort",
    "abs",
    "accept",
    "accept4",
    "acct",
    "alarm",
    "alphasort",
    "asctime",
    "atof",
    "atoi",
    "atol",
    "atoll",
    "bcopy",
    "bindresvport",
    "bsearch",
    "c16rtomb",
    "c32rtomb",
    "c8rtomb",
    "capget",
    "capset",
    "catclose",
    "catgets",
    "catopen",
    "cfgetispeed",
    "cfgetospeed",
    "cfmakeraw",
    "cfsetispeed",
    "cfsetospeed",
    "cfsetspeed",
    "chflags",
    "chroot",
    "clearerr",
    "clock",
    "closelog",
    "confstr",
    "ctermid",
    "ctime",
    "close",
    "cuserid",
    "daemon",
    "difftime",
    "dirfd",
    "dirname",
    "div",
    "dprintf",
    "drand48",
    "dysize",
    "ecvt",
    "endaliasent",
    "endfsent",
    "endgrent",
    "endhostent",
    "endnetent",
    "endnetgrent",
    "endprotoent",
    "endpwent",
    "endrpcent",
    "endservent",
    "endsgent",
    "endspent",
    "endusershell",
    "endutxent",
    "erand48",
    "err",
    "errx",
    "eventfd",
    "execl",
    "execle",
    "execlp",
    "execv",
    "execveat",
    "execvp",
    "exit",
    "fallocate",
    "fallocate64",
    "fchflags",
    "fchmodat",
    "fchownat",
    "fcntl",
    "fcvt",
    "fdatasync",
    "fexecve",
    "ffs",
    "ffsll",
    "fgetgrent",
    "fgetpwent",
    "fgetsgent",
    "fgetspent",
    "fgetws",
    "fgetxattr",
    "flistxattr",
    "fmtmsg",
    "fprintf",
    "fputc",
    "fputwc",
    "fputws",
    "free",
    "freeaddrinfo",
    "fremovexattr",
    "freopen",
    "freopen64",
    "fscanf",
    "fsconfig",
    "fseek",
    "fsetxattr",
    "fsmount",
    "fsopen",
    "fspick",
    "fsync",
    "ftime",
    "ftok",
    "ftw",
    "ftw64",
    "fwide",
    "fwscanf",
    "gcvt",
    "getaddrinfo",
    "getaliasbyname",
    "getaliasent",
    "getchar",
    "getdate",
    "getdirentries",
    "getdirentries64",
    "getdomainname",
    "getentropy",
    "getenv",
    "getfsent",
    "getfsfile",
    "getfsspec",
    "getgrent",
    "getgrgid",
    "getgrnam",
    "getgrouplist",
    "gethostbyaddr",
    "gethostbyname",
    "gethostbyname2",
    "gethostent",
    "gethostid",
    "getipv4sourcefilter",
    "getloadavg",
    "getlogin",
    "getmntent",
    "getnameinfo",
    "getnetbyaddr",
    "getnetbyname",
    "getnetent",
    "getnetgrent",
    "getopt",
    "getpass",
    "getpgrp",
    "getprotobyname",
    "getprotobynumber",
    "getprotoent",
    "getpwent",
    "getpwnam",
    "getpwuid",
    "getrpcbyname",
    "getrpcbynumber",
    "getrpcent",
    "getservbyname",
    "getservbyport",
    "getservent",
    "getsgent",
    "getsgnam",
    "getsid",
    "getsourcefilter",
    "getspent",
    "getspnam",
    "getsubopt",
    "getusershell",
    "getutmp",
    "getutmpx",
    "getutxent",
    "getutxid",
    "getutxline",
    "getw",
    "getwchar",
    "getwd",
    "getxattr",
    "globfree",
    "globfree64",
    "gmtime",
    "grantpt",
    "gtty",
    "hcreate",
    "herror",
    "hsearch",
    "hstrerror",
    "htonl",
    "htons",
    "iconv",
    "initgroups",
    "innetgr",
    "insque",
    "ioperm",
    "iopl",
    "iruserok",
    "isalnum",
    "isalpha",
    "isascii",
    "isblank",
    "iscntrl",
    "isdigit",
    "isfdtype",
    "isgraph",
    "islower",
    "isprint",
    "ispunct",
    "isspace",
    "isupper",
    "isxdigit",
    "jrand48",
    "killpg",
    "klogctl",
    "l64a",
    "labs",
    "lchmod",
    "lcong48",
    "ldiv",
    "lfind",
    "lgetxattr",
    "linkat",
    "listen",
    "listxattr",
    "llabs",
    "lldiv",
    "llistxattr",
    "localtime",
    "lockf",
    "lrand48",
    "lremovexattr",
    "lsearch",
    "lsetxattr",
    "malloc",
    "mblen",
    "mbrtoc16",
    "mbrtoc32",
    "mbrtoc8",
    "mbstowcs",
    "mbtowc",
    "mcheck",
    "mcheck_check_all",
    "memcmp",
    "memcpy",
    "memfrob",
    "memmove",
    "memset",
    "mincore",
    "mkdirat",
    "mkdtemp",
    "mkfifo",
    "mkfifoat",
    "mkostemp",
    "mkostemp64",
    "mkostemps",
    "mkostemps64",
    "mkstemp",
    "mkstemp64",
    "mkstemps",
    "mkstemps64",
    "mktime",
    "mlock",
    "mlock2",
    "mlockall",
    "mprobe",
    "mrand48",
    "msgget",
    "msync",
    "mtrace",
    "munlock",
    "munlockall",
    "muntrace",
    "nice",
    "nrand48",
    "open",
    "openlog",
    "open64",
    "perror",
    "ppoll",
    "preadv",
    "preadv2",
    "preadv64",
    "preadv64v2",
    "printf",
    "prlimit",
    "prlimit64",
    "psiginfo",
    "psignal",
    "ptrace",
    "ptsname",
    "putchar",
    "putenv",
    "putgrent",
    "putpwent",
    "putsgent",
    "putspent",
    "pututxline",
    "putw",
    "putwc",
    "putwchar",
    "pwritev",
    "pwritev2",
    "pwritev64",
    "pwritev64v2",
    "qecvt",
    "qfcvt",
    "qgcvt",
    "qsort",
    "quotactl",
    "raise",
    "rand",
    "rcmd",
    "read",
    "readlinkat",
    "realloc",
    "reboot",
    "remove",
    "removexattr",
    "remque",
    "rename",
    "rewind",
    "rexec",
    "rpmatch",
    "rresvport",
    "ruserok",
    "ruserpass",
    "scandir",
    "scandirat64",
    "scanf",
    "seed48",
    "seekdir",
    "semget",
    "semop",
    "sendfile",
    "sendfile64",
    "setaliasent",
    "setbuf",
    "setdomainname",
    "setegid",
    "seteuid",
    "setfsent",
    "setfsgid",
    "setfsuid",
    "setgrent",
    "setgroups",
    "sethostent",
    "sethostid",
    "sethostname",
    "setipv4sourcefilter",
    "setjmp",
    "setlinebuf",
    "setlocale",
    "setlogin",
    "setlogmask",
    "setnetent",
    "setnetgrent",
    "setns",
    "setpgrp",
    "setprotoent",
    "setpwent",
    "setrpcent",
    "setservent",
    "setsgent",
    "setsourcefilter",
    "setspent",
    "setusershell",
    "setutxent",
    "setxattr",
    "sgetsgent",
    "sgetspent",
    "shmat",
    "shmdt",
    "shmget",
    "sigaddset",
    "sigandset",
    "sigdelset",
    "sigemptyset",
    "sigfillset",
    "siggetmask",
    "sighold",
    "sigignore",
    "siginterrupt",
    "sigisemptyset",
    "sigismember",
    "signalfd",
    "sigorset",
    "sigpending",
    "sigrelse",
    "sigset",
    "sigstack",
    "sockatmark",
    "splice",
    "sprintf",
    "srand48",
    "sscanf",
    "statx",
    "strcat",
    "strchr",
    "strcmp",
    "strcoll",
    "strcpy",
    "strcspn",
    "strerror",
    "strfmon",
    "strfromd",
    "strfromf",
    "strfromf128",
    "strfroml",
    "strfry",
    "strftime",
    "strlen",
    "strncat",
    "strncmp",
    "strncpy",
    "strnlen",
    "strpbrk",
    "strptime",
    "strrchr",
    "strsignal",
    "strspn",
    "strstr",
    "strtod",
    "strtof",
    "strtof128",
    "strtok",
    "strtold",
    "strxfrm",
    "stty",
    "swab",
    "swprintf",
    "swscanf",
    "symlinkat",
    "sync",
    "syncfs",
    "syscall",
    "syslog",
    "tcflow",
    "tcflush",
    "tcgetpgrp",
    "tcgetsid",
    "tcsendbreak",
    "tcsetattr",
    "tee",
    "telldir",
    "tempnam",
    "timegm",
    "tmpfile64",
    "tmpnam",
    "toascii",
    "tolower",
    "toupper",
    "ttyname",
    "ttyslot",
    "ualarm",
    "ungetc",
    "ungetwc",
    "unlinkat",
    "unlockpt",
    "unshare",
    "updwtmpx",
    "usleep",
    "utime",
    "utmpxname",
    "verr",
    "verrx",
    "versionsort",
    "vfprintf",
    "vhangup",
    "vlimit",
    "vmsplice",
    "vprintf",
    "vwarn",
    "vwarnx",
    "vwprintf",
    "vwscanf",
    "warn",
    "warnx",
    "wcscspn",
    "wcsdup",
    "wcsftime",
    "wcsncat",
    "wcsncmp",
    "wcspbrk",
    "wcsrchr",
    "wcsspn",
    "wcsstr",
    "wcstod",
    "wcstof",
    "wcstof128",
    "wcstok",
    "wcstold",
    "wcstombs",
    "wcswidth",
    "wcsxfrm",
    "wctob",
    "wctomb",
    "wcwidth",
    "wordexp",
    "wordfree",
    "wprintf",
    "write",
    "wscanf",
    "__errno_location",
];
