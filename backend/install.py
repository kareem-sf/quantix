import subprocess

result = subprocess.run(
    ["D:\AI Work\quantix\backend\.venv\Scripts\python", "-m", "pip", "install", "-e", ".[test]"],
    capture_output=True,
    text=True,
    cwd="D:\AI Work\quantix\backend",
)
print("STDOUT:", result.stdout)
print("STDERR:", result.stderr)
