(define (teleport a b)
    (set-position a (position-entity b)))

(define (zob) (set-faction (player-entity) 'zob))