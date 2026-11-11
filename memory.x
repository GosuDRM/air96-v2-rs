MEMORY
{
    /* Last 8K (0x0801_E000..) holds settings: see src/config/eeprom.rs. */
    FLASH : ORIGIN = 0x08000000, LENGTH = 120K
    RAM   : ORIGIN = 0x20000000, LENGTH = 16K
}
