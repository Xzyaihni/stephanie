(define (entity? x)
    (eq? (car x) 'entity))

(define (entity->position x)
    (if (entity? x)
        (position-entity x)
        x))

(define (teleport a b)
    (set-position a (entity->position b)))

(define (move a amount)
    (set-position
        a
        (map
            (lambda (x) (+ (car x) (cdr x)))
            (zip
                (position-entity a)
                amount))))

(define (distance a b)
    (sqrt
        (fold
            +
            0
            (map
                square
                (map
                    (lambda (x) (- (car x) (cdr x)))
                    (zip (entity->position a) (entity->position b)))))))

(define (entities-near entity near-distance)
    (filter
        (lambda (x) (< (distance entity x) near-distance))
        (all-entities)))

(define (zob) (set-faction (player-entity) 'zob))

(define (noclip state)
    (set-floating (player-entity) state)
    (set-ghost (player-entity) state))
