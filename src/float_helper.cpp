#include <cstdio>
#include <cfenv>


extern "C" void float_excepts()
{
    if (feenableexcept(FE_INVALID | FE_OVERFLOW | FE_DIVBYZERO) != 0)
    {
        printf("error setting feenableexcept\n");
    }
}
