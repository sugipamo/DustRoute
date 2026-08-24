from __future__ import annotations

import importlib
import inspect


MODULES=(
    "test_logic_dag",
    "test_electrical",
    "test_cells",
    "test_connectivity",
    "test_routing",
    "test_compiler",
    "test_optimization",
    "test_minecraft",
)


def all_tests():
    tests=[]
    package=__package__
    for module_name in MODULES:
        module=importlib.import_module(f"{package}.{module_name}")
        tests.extend(
            value
            for name,value in vars(module).items()
            if name.startswith("test_") and inspect.isfunction(value) and value.__module__ == module.__name__
        )
    return tuple(tests)


def main():
    tests=all_tests()
    for test in tests:
        test()
        print("PASS",test.__name__)
    print(f"ALL {len(tests)} TESTS PASS")


if __name__=="__main__":
    main()
