const util = require('util')

let tick = 0

console.log('RYAN!')

console.log('start')

// process.stderr.write("I will goto the STDERR")

setInterval(() => {
    if (tick === 9) {
        console.log('Ryan tick 9')
    } else {
        console.log(util.inspect({tick}, {colors: true}))
    }

    if (tick % 12 === 0) {
        console.log('RYAN HERE')
    }


    tick += 1
}, 1000)
