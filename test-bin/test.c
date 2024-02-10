#include <stdio.h>

int g_global = 0;

void stuff(int* var) {
    *var = g_global;
}

int main() {
    printf("hi\n");

    asm("int3");

    g_global = 0x1234;
    int x;
    x = 0x5678;

    stuff(&x);

    asm("int3");

    printf("x: %i, g_global: %i, &x: %p, &g_global: %p\n", x, g_global, &x, &g_global);

    return 0x1337;
}