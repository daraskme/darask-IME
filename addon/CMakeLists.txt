cmake_minimum_required(VERSION 3.16)
project(fcitx5-darask VERSION 0.1.0 LANGUAGES CXX)

find_package(ECM REQUIRED NO_MODULE)
set(CMAKE_MODULE_PATH "${ECM_MODULE_PATH}")
find_package(Fcitx5Core REQUIRED)
include(FeatureSummary)
include(GNUInstallDirs)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_POSITION_INDEPENDENT_CODE ON)

# Rust static library (built by cargo; path given by the caller)
set(DARASK_FFI_LIB "" CACHE FILEPATH "path to libdarask_ffi.a")
set(DARASK_DATA_DIR "${CMAKE_INSTALL_FULL_DATADIR}/darask-ime" CACHE PATH "runtime data dir")

add_library(darask MODULE darask.cpp)
target_compile_definitions(darask PRIVATE DARASK_DATA_DIR="${DARASK_DATA_DIR}")
target_link_libraries(darask PRIVATE Fcitx5::Core ${DARASK_FFI_LIB} ${CMAKE_DL_LIBS} pthread)
set_target_properties(darask PROPERTIES PREFIX "")
install(TARGETS darask LIBRARY DESTINATION "${CMAKE_INSTALL_LIBDIR}/fcitx5")

install(FILES darask-addon.conf DESTINATION "${CMAKE_INSTALL_DATADIR}/fcitx5/addon" RENAME darask.conf)
install(FILES darask-im.conf DESTINATION "${CMAKE_INSTALL_DATADIR}/fcitx5/inputmethod" RENAME darask.conf)

# End-to-end test driver: loads the real addon in a real fcitx5 Instance.
find_package(Fcitx5ModuleTestFrontend QUIET)
if(TARGET Fcitx5::Module::TestFrontend)
  add_executable(darask-e2e test/e2e.cpp)
  target_link_libraries(darask-e2e PRIVATE Fcitx5::Core Fcitx5::Module::TestFrontend)
  install(TARGETS darask-e2e DESTINATION "${CMAKE_INSTALL_LIBEXECDIR}")
endif()
