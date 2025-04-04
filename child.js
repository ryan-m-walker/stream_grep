// Child process that emits logs at specified intervals
const util = require('util');
const name = process.argv[2] || 'unknown';
const interval = parseInt(process.argv[3]) || 1000;

console.log(`Process ${name} started with PID:${process.pid} and interval ${interval}ms`);

let counter = 0;

// Emit logs at the specified interval
const timer = setInterval(() => {
  counter++;
  
  // Occasionally emit a warning or error level log
  if (counter % 5 === 0) {
    console.log(`WARNING: This is warning log #${counter} from ${name}`);
  } else if (counter % 10 === 0) {
    console.error(`ERROR: This is error log #${counter} from ${name}`);
  } else {
    console.log(util.inspect({counter, name}, {colors: true}));
  }

  // Occasionally emit multiple logs in a burst
  if (counter % 7 === 0) {
    console.log(`${name} is emitting a burst of logs:`);
    console.log(`${name} burst log 1`);
    console.log(`${name} burst log 2`);
    console.log(`${name} burst log 3`);
  }
  
  // Special message occasionally
  if (counter % 12 === 0) {
    console.log(`IMPORTANT MESSAGE FROM ${name}`);
  }
}, interval);

// Clean shutdown
process.on('SIGINT', () => {
  console.log(`Process ${name} received SIGINT`);
  clearInterval(timer);
  process.exit(0);
});

process.on('SIGTERM', () => {
  console.log(`Process ${name} received SIGTERM`);
  clearInterval(timer);
  process.exit(0);
});