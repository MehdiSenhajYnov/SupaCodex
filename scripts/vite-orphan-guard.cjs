const parentPid = Number.parseInt(
  process.env.SUPACODEX_VITE_GUARD_PARENT_PID ?? "",
  10,
);

function parentStillAlive(pid) {
  if (!Number.isInteger(pid) || pid <= 0) {
    return true;
  }

  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error?.code === "EPERM";
  }
}

if (Number.isInteger(parentPid) && parentPid > 0) {
  const interval = setInterval(() => {
    if (!parentStillAlive(parentPid)) {
      process.exit(0);
    }
  }, 1500);

  if (typeof interval.unref === "function") {
    interval.unref();
  }
}

process.on("disconnect", () => {
  process.exit(0);
});
