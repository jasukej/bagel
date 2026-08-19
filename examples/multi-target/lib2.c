extern int add(int a, int b);

int lib2_triple(int x) {
    return add(add(x, x), x);
}
