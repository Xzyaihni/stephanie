(define (entity? x)
    (eq? (car x) 'entity))

(define (entity->position x)
    (if (entity? x)
        (position-entity x)
        x))

(define (position-combine f a b)
    (map
        (lambda (x) (f (car x) (cdr x)))
        (zip a b)))

(define (position-add a b)
    (position-combine + a b))

(define (teleport a b)
    (set-position a (entity->position b)))

(define (move a amount)
    (set-position
        a
        (position-add (position-entity a) amount)))

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

(define (has-component entity component)
    (not (null? (format-component entity component))))

(define (print-component entity component)
    (display (format-component entity component)))

(define (string-ref s i)
    (vector-ref (cdr s) i))

(define (vector-prefix? xs other)
    (if (< (vector-length xs) (vector-length other))
        #f
        (all
            (map
                (lambda (i) (eq? (vector-ref xs i) (vector-ref other i)))
                (counter (vector-length other))))))

(define (vector-infix? xs other)
    (define (any-meets f i limit)
        (if (= i limit)
            #f
            (if (f i) #t (any-meets f (+ i 1) limit))))
    (let ((limit (- (vector-length xs) (vector-length other))))
        (if (< limit 0)
            #f
            (any-meets
                (lambda
                    (start)
                    (all
                        (map
                            (lambda (i) (eq? (vector-ref xs (+ start i)) (vector-ref other i)))
                            (counter (vector-length other)))))
                0
                (+ limit 1)))))

(define (string-infix? xs other)
    (vector-infix? (cdr xs) (cdr other)))

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
