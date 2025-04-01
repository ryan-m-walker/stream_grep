let tick = 0

setInterval(() => {
    console.log('tick', tick++)

    if (tick > 4) {
        throw new Error('test')
    }
}, 1000)
