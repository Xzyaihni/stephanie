(define (teleport a b)
    (set-position a (position-entity b)))

(define (distance a b)
    (sqrt
        (fold
            +
            0
            (map
                square
                (map - (zip a b))))))

(define (zob) (set-faction (player-entity) 'zob))

(define (noclip state)
    (set-floating (player-entity) state)
    (set-ghost (player-entity) state))
