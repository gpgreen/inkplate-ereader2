// .embuild/espressif/esp-idf/v5.1.1/examples/system/freertos/real_time_stats/main

#define __GET_TASK_INFO__ 1
#include <stdio.h>
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "get_task_info.hpp"

static char const * TAG = "GetTaskInfo";

#define ARRAY_SIZE_OFFSET     5

EXTERNC esp_err_t get_task_info(int xTicksToWait)
{
    TaskStatus_t *start_array = NULL, *end_array = NULL;
    UBaseType_t start_array_size, end_array_size;
    uint32_t start_run_time, end_run_time;
    esp_err_t ret;
    uint32_t total_elapsed_time = 0;

    //Allocate array to store current task states
    start_array_size = uxTaskGetNumberOfTasks() + ARRAY_SIZE_OFFSET;
    start_array = static_cast<TaskStatus_t*>(malloc(sizeof(TaskStatus_t) * start_array_size));
    if (start_array == NULL) {
        ret = ESP_ERR_NO_MEM;
        goto exit;
    }
    //Get current task states
    start_array_size = uxTaskGetSystemState(start_array, start_array_size, &start_run_time);
    if (start_array_size == 0) {
        ret = ESP_ERR_INVALID_SIZE;
        goto exit;
    }

    vTaskDelay(xTicksToWait);

    //Allocate array to store tasks states post delay
    end_array_size = uxTaskGetNumberOfTasks() + ARRAY_SIZE_OFFSET;
    end_array = static_cast<TaskStatus_t*>( malloc(sizeof(TaskStatus_t) * end_array_size));
    if (end_array == NULL) {
        ret = ESP_ERR_NO_MEM;
        goto exit;
    }
    //Get post delay task states
    end_array_size = uxTaskGetSystemState(end_array, end_array_size, &end_run_time);
    if (end_array_size == 0) {
        ret = ESP_ERR_INVALID_SIZE;
        goto exit;
    }

    //Calculate total_elapsed_time in units of run time stats clock period.
    total_elapsed_time = (end_run_time - start_run_time);
    if (total_elapsed_time == 0) {
        ret = ESP_ERR_INVALID_STATE;
        goto exit;
    }

    printf("| Task | Run Time | Percentage\n");
    //Match each task in start_array to those in the end_array
    for (int i = 0; i < start_array_size; i++) {
        int k = -1;
        for (int j = 0; j < end_array_size; j++) {
            if (start_array[i].xHandle == end_array[j].xHandle) {
                k = j;
                //Mark that task have been matched by overwriting their handles
                start_array[i].xHandle = NULL;
                end_array[j].xHandle = NULL;
                break;
            }
        }
        //Check if matching task found
        if (k >= 0) {
            uint32_t task_elapsed_time = end_array[k].ulRunTimeCounter - start_array[i].ulRunTimeCounter;
            uint32_t percentage_time = (task_elapsed_time * 100UL) / (total_elapsed_time * portNUM_PROCESSORS);
//            printf("| %s | %"PRIu32" | %"PRIu32"%%\n", start_array[i].pcTaskName, task_elapsed_time, percentage_time);
            printf("| %s | %ld | %ld\n", start_array[i].pcTaskName, task_elapsed_time, percentage_time);
        }
    }

    //Print unmatched tasks
    for (int i = 0; i < start_array_size; i++) {
        if (start_array[i].xHandle != NULL) {
            printf("| %s | Deleted\n", start_array[i].pcTaskName);
        }
    }
    for (int i = 0; i < end_array_size; i++) {
        if (end_array[i].xHandle != NULL) {
            printf("| %s | Created\n", end_array[i].pcTaskName);
        }
    }
    ret = ESP_OK;

exit:    //Common return path
    free(start_array);
    free(end_array);
    return ret;

    // char *write_buffer = pvPortMalloc( 2048 );
    // int unwritten = 2048;
    // if (write_buffer == NULL) {
    //     return NULL;
    // }

    // // Make sure the write buffer does not contain a string.
    // *write_buffer = 0x00;

    // // Take a snapshot of the number of tasks in case it changes while this
    // // function is executing.
    // uxArraySize = uxTaskGetNumberOfTasks();

    // // Allocate a TaskStatus_t structure for each task.  An array could be
    // // allocated statically at compile time.
    // pxTaskStatusArray = pvPortMalloc( uxArraySize * sizeof( TaskStatus_t ) );

    // if( pxTaskStatusArray != NULL )
    // {
    //     // Generate raw status information about each task.
    //     uxArraySize = uxTaskGetSystemState( pxTaskStatusArray, uxArraySize, &ulTotalRunTime );

    //     // For percentage calculations.
    //     ulTotalRunTime /= 100UL;

    //     // Avoid divide by zero errors.
    //     if( ulTotalRunTime > 0 )
    //     {
    //         // For each populated position in the pxTaskStatusArray array,
    //         // format the raw data as human readable ASCII data
    //         for( x = 0; x < uxArraySize; x++ )
    //         {
    //             // What percentage of the total run time has the task used?
    //             // This will always be rounded down to the nearest integer.
    //             // ulTotalRunTimeDiv100 has already been divided by 100.
    //             ulStatsAsPercentage = pxTaskStatusArray[ x ].ulRunTimeCounter / ulTotalRunTime;

    //             if( ulStatsAsPercentage > 0UL )
    //             {
    //                 snprintf( write_buffer, unwritten, "%s\t\t%lu\t\t%lu%%\t%lu\r\n", pxTaskStatusArray[ x ].pcTaskName, pxTaskStatusArray[ x ].ulRunTimeCounter, ulStatsAsPercentage, pxTaskStatuArray[ x ].usStackHighWaterMark );
    //             }
    //             else
    //             {
    //                 // If the percentage is zero here then the task has
    //                 // consumed less than 1% of the total run time.
    //                 snprintf( write_buffer, unwritten, "%s\t\t%lu\t\t<1%%]\t%lu\r\n", pxTaskStatusArray[ x ].pcTaskName, pxTaskStatusArray[ x ].ulRunTimeCounter, pxTaskStatuArray[ x ].usStackHighWaterMark );
    //             }

    //             write_buffer += strlen( ( char * ) write_buffer );
    //             unwritten = bufsize - strlen((char*) write_buffer);
    //             if (unwritten <= 20) {
    //                 break;
    //             }
    //         }
    //     }

    //     // The array is no longer needed, free the memory it consumes.
    //     vPortFree( pxTaskStatusArray );
    // }
    // return write_buffer;
}
