const { spawn } = require('child_process');
const path = require('path');

// Create and manage child processes
function createChildProcess(name, interval) {
  const child = spawn('node', [path.join(__dirname, 'child.js'), name, interval]);
  
  child.stdout.on('data', (data) => {
    // Prefix child logs with their name
    const messages = data.toString().trim().split('\n');
    messages.forEach(msg => {
      console.log(`[${name}] ${msg}`);
    });
  });
  
  // Log child PID when it's created
  console.log(`[PARENT] Created child process ${name} with PID: ${child.pid}`);

  child.stderr.on('data', (data) => {
    console.error(`[${name} ERROR] ${data.toString().trim()}`);
  });

  child.on('close', (code) => {
    console.log(`[PARENT] Child process ${name} exited with code ${code}`);
  });

  return child;
}

// Parent process
console.log(`[PARENT PID:${process.pid}] Starting parent process`);
console.log('[PARENT] Creating child processes');

// Create multiple child processes with different intervals
const child1 = createChildProcess('app1', 1000);
const child2 = createChildProcess('app2', 1500);
const child3 = createChildProcess('app3', 2000);

// Watch for OS Signals and propagate to children
// process.on('SIGINT', () => {
//   console.log('[PARENT] Received SIGINT. Stopping children...');
//   child1.kill();
//   child2.kill();
//   child3.kill();
//   process.exit(0);
// });
//
// process.on('SIGTERM', () => {
//   console.log('[PARENT] Received SIGTERM. Stopping children...');
//   child1.kill();
//   child2.kill();
//   child3.kill();
//   process.exit(0);
// });

console.log('[PARENT] Parent process setup complete');
