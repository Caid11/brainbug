#include <stdio.h>
#include <stdlib.h>

extern void bf_main( unsigned char* tape );

int main(int argc, char** argv)
{
    unsigned char* tape = calloc(4000000, sizeof(char));
    bf_main( tape );
    free(tape);
    printf("Exited successfully\n");
}
