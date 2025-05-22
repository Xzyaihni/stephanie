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

(define (fold-entities f start)
    (define query (all-entities-query))
    (define (rest-entities state)
        (let ((next (query-entity-next query)))
            (if (null? next)
                state
                (rest-entities (f next state)))))
    (rest-entities start))

(define (for-each-entity f)
    (define query (all-entities-query))
    (define (rest-entities)
        (let ((next (query-entity-next query)))
            (if (null? next)
                '()
                (begin (f next) (rest-entities)))))
    (rest-entities))

(define (filtered-entities f)
    (define query (all-entities-query))
    (define (rest-entities)
        (let ((next (query-entity-next query)))
            (if (null? next)
                '()
                (if (f next)
                    (cons next (rest-entities))
                    (rest-entities)))))
    (rest-entities))

(define (included-with component lst)
    (filter (lambda (x) (has-component x component)) lst))

(define (excluded-with component lst)
    (filter (lambda (x) (not (has-component x component))) lst))

(define (entities-near entity near-distance)
    (filtered-entities (lambda (x) (< (distance entity x) near-distance))))

(define (zob) (set-faction (player-entity) 'zob))

(define (noclip state)
    (set-floating (player-entity) state)
    (set-ghost (player-entity) state))
