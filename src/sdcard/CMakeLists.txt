set (lib_sources
  sd_card.cpp
  get_task_info.cpp
  )

idf_component_register(
  SRCS ${lib_sources}
  INCLUDE_DIRS .
  REQUIRES fatfs
)

# control component compilation
target_compile_definitions(${COMPONENT_LIB} PUBLIC INKPLATE_6PLUS)
#target_compile_options(${COMPONENT_LIB} PRIVATE -std=gnu++17)
