#include <stdio.h>
#include <stdlib.h>
#include <fcntl.h>
#include <io.h>

extern void bf_main( unsigned char* tape );

// int getchar_debug( void )
// {
//     static int eof_returned = 0;
//     char res = getchar();
//     printf("READ: %u\n", (unsigned char)res);
//     if( res != -1 )
//         eof_returned = 0;
//     else
//         eof_returned++;
// 
//     if( eof_returned > 50 )
//         exit(0);
//     return res;
// }

int main(int argc, char** argv)
{
    // Don't interpret ctrl z as EOF.
    _setmode(0,_O_BINARY);

    unsigned char* tape = calloc(4000000, sizeof(char));
    bf_main( tape + 2000000 );
    free(tape);
    fprintf(stderr, "Exited successfully\n");
}
