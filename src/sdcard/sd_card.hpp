#pragma once

#ifdef __cplusplus
#define EXTERNC extern "C"
#else
#define EXTERNC
#endif

//#include "driver/sdmmc_host.h"
//#include "driver/sdspi_host.h"

//EXTERNC sdmmc_host_t sdspi_host_default();
//EXTERNC sdspi_slot_config_t slot_config_default();
EXTERNC bool sdcard_setup();
