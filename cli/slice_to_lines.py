import json
import sys

if __name__ == "__main__":
    arguments = sys.argv
    if len(arguments) != 2:
        print("Please give JSON file as input")
        sys.exit()
    with open(arguments[1]) as f:
        data = json.loads(f.read())
        lines = set()
        for stmt in data["statements"]:
            lines.add(stmt["line"])
        sorted_lines = [l for l in lines]
        sorted_lines.sort()
        print(sorted_lines)