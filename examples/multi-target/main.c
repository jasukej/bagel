#include <stdio.h>

extern int lib1_double(int x);
extern int lib2_triple(int x);

int main(void) {
    printf("lib1_double(4) = %d\n", lib1_double(4));
    printf("lib2_triple(4) = %d\n", lib2_triple(4));
    return 0;
}
