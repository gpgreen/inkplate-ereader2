#define __SD_CARD__ 1
#include "sd_card.hpp"
#include "esp_log.h"

#include "esp_vfs_fat.h"
#include "sdmmc_cmd.h"

static char const * TAG = "sdcard";

#define MOUNT_POINT "/sdcard"

enum SDCardState {
    UNINITIALIZED,
    INITIALIZED,
    FAILED
};

static SDCardState state = SDCardState::UNINITIALIZED;

EXTERNC sdmmc_host_t sdspi_host_default()
{
    sdmmc_host_t host = SDSPI_HOST_DEFAULT();
    return host;
}


EXTERNC sdspi_device_config_t slot_config_default()
{
  sdspi_device_config_t slot_config = SDSPI_DEVICE_CONFIG_DEFAULT();
  //slot_config.gpio_cs = PIN_NUM_CS;
  //slot_config.host_id = host.slot;
  return slot_config;
}


EXTERNC bool sdcard_setup()
{
  sdmmc_card_t* card;
  esp_err_t ret;
  const char mount_point[] = MOUNT_POINT;

  switch (state)
  {
  case SDCardState::INITIALIZED:
    ESP_LOGI(TAG, "SD card is already initialized");
    return true;
  case SDCardState::FAILED:
    ESP_LOGI(TAG, "SD card setup is recently failed");
    return false;
  case SDCardState::UNINITIALIZED:
    ESP_LOGI(TAG, "Setup SD card");
  }

  static const gpio_num_t PIN_NUM_MISO = GPIO_NUM_12;
  static const gpio_num_t PIN_NUM_MOSI = GPIO_NUM_13;
  static const gpio_num_t PIN_NUM_CLK  = GPIO_NUM_14;
  static const gpio_num_t PIN_NUM_CS   = GPIO_NUM_15;

  esp_vfs_fat_sdmmc_mount_config_t mount_config = {
    .format_if_mount_failed = false,
    .max_files = 5,
    .allocation_unit_size = 16 * 1024
  };

  state = SDCardState::FAILED;

  // By default, SD card frequency is initialized to SDMMC_FREQ_DEFAULT (20MHz)
  // For setting a specific frequency, use host.max_freq_khz (range 400kHz - 20MHz for SDSPI)
  // Example: for fixed frequency of 10MHz, use host.max_freq_khz = 10000;
  sdmmc_host_t host = SDSPI_HOST_DEFAULT();

  spi_bus_config_t bus_cfg = {
      .mosi_io_num = PIN_NUM_MOSI,
      .miso_io_num = PIN_NUM_MISO,
      .sclk_io_num = PIN_NUM_CLK,
      .quadwp_io_num = -1,
      .quadhd_io_num = -1,
      .max_transfer_sz = 4000,
  };
  ret = spi_bus_initialize(static_cast<spi_host_device_t>(host.slot), &bus_cfg, SDSPI_DEFAULT_DMA);
  if (ret != ESP_OK) {
      ESP_LOGE(TAG, "Failed to initialize bus.");
      return false;
  }

  // This initializes the slot without card detect (CD) and write protect (WP) signals.
  // Modify slot_config.gpio_cd and slot_config.gpio_wp if your board has these signals.
  sdspi_device_config_t slot_config = SDSPI_DEVICE_CONFIG_DEFAULT();
  slot_config.gpio_cs = PIN_NUM_CS;
  slot_config.host_id = static_cast<spi_host_device_t>(host.slot);

  ESP_LOGI(TAG, "Mounting filesystem at %s", mount_point);
  ret = esp_vfs_fat_sdspi_mount(mount_point, &host, &slot_config, &mount_config, &card);

  if (ret != ESP_OK) {
      if (ret == ESP_FAIL) {
          ESP_LOGE(TAG, "Failed to mount filesystem. "
                   "If you want the card to be formatted, set the CONFIG_EXAMPLE_FORMAT_IF_MOUNT_FAILED menuconfig option.");
      } else {
          ESP_LOGE(TAG, "Failed to initialize the card (%s). "
                   "Make sure SD card lines have pull-up resistors in place.", esp_err_to_name(ret));
      }
      return false;
  }
  ESP_LOGI(TAG, "Filesystem mounted");

  // Card has been initialized, print its properties
  sdmmc_card_print_info(stdout, card);

  state = SDCardState::INITIALIZED;

  return true;
}

// EXTERNC bool sdcard_release()
// {
//     // All done, unmount partition and disable SPI peripheral
//     esp_vfs_fat_sdcard_unmount(mount_point, card);
//     ESP_LOGI(TAG, "Card unmounted");

//     //deinitialize the bus after all devices are removed
//     spi_bus_free(host.slot);
// }
