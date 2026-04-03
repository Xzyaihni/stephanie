(define (any-of xs)
    (list-ref xs (random-integer (length xs))))

(define (drop-between f start end)
    (map (lambda (_) (f)) (counter (random-integer-between start (+ end 1)))))

(define (drop-between-less f start end rate)
    (cond
        ((= start 0) (if (rate) (drop-between-less f 1 end rate) '()))
        ((= start 1) (maybe-multiple f rate (+ (- end start) 1)))
        (else (cons (f) (drop-between-less f (- start 1) (- end 1) rate)))))

(define (maybe-multiple f rate limit)
    (define (inner current limit)
        (if (> limit 0)
            (if (rate)
                (inner (cons (f) current) (- limit 1))
                current)
            current))
    (inner (list (f)) (- limit 1)))

(define (drop-rate x total)
    (lambda ()
        (< (random-integer total) x)))

(define (and-maybe xs rate x)
    (if (rate)
        (cons x xs)
        xs))

(define (and-always xs x) (cons x xs))

(define (either a b) (if (random-bool) a b))

(define (standard-drops items)
    (maybe-multiple (lambda () (any-of items)) (drop-rate 1 3) 3))

(define (points-drops items points)
    (define item-name car)
    (define (item-cost x) (car (cdr x)))
    (let ((possible (filter (lambda (x) (<= (item-cost x) points)) items)))
        (if (null? possible)
            '()
            (let ((picked (any-of possible)))
                (cons (item-name picked) (points-drops possible (- points (item-cost picked))))))))

(define trash '(rock bottle short_stick stick branch duct_tape))
