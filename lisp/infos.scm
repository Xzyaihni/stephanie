(load-once "standard.scm")

(define tile-size 0.1)

(define (entity? x)
    (eq? (car x) 'entity))

(define (local? x) (car (cdr x)))

(define transform-position car)

(define (transform-scale transform)
    (list-ref transform 1))

(define (transform-rotation transform)
    (list-ref transform 2))

(define (entity->position entity)
    (transform-position (entity-transform entity)))

(define (entity->scale entity)
    (transform-scale (entity-transform entity)))

(define (entity->rotation entity)
    (transform-rotation (entity-transform entity)))

(define (position-combine f a b)
    (map
        (lambda (x) (f (car x) (cdr x)))
        (zip a b)))

(define (position-add a b)
    (position-combine + a b))

(define (rotate-point p a)
    (let ((na (* a -1)))
        (let ((asin (sin na)) (acos (cos na)) (px (list-ref p 0)) (py (list-ref p 1)))
            (cons
                (+ (* acos px) (* asin py))
                (cons
                    (+ (* (* asin -1) px) (* acos py))
                    (cdr (list-tail p 1)))))))

(define (teleport a b)
    (set-position a (entity->position b)))

(define (move a amount)
    (set-position
        a
        (position-add (entity->position a) amount)))

(define (distance a b)
    (let ((a-pos (entity->position a)) (b-pos (entity->position b)))
        (if (or (null? a-pos) (null? b-pos))
            (/ 1.0 0.0)
            (sqrt
                (fold
                    +
                    0
                    (map
                        square
                        (map
                            (lambda (x) (- (car x) (cdr x)))
                            (zip a-pos b-pos))))))))

(define tile-air? null?)

(define (remove-tile pos) (set-tile pos 'air))

(define spawn-enemy-clean (lambda args
    (let ((enemy (car args)) (pos (car (cdr args))) (rest (cdr (cdr args))))
        (begin
            (remove-tile (closest-tile pos))
            (if (null? rest)
                (spawn-enemy enemy pos)
                (spawn-enemy enemy pos (car rest)))))))
