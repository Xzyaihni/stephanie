(define (any-of xs)
    (list-ref xs (random-integer (length xs))))

(define (drop-between f start end)
    (map (lambda (_) (f)) (counter (random-integer-between start (+ end 1)))))

(define trash '(rock bottle branch duct_tape))

(define (zob)
    '(rock bottle branch))

(define (runner)
    '(rock bottle branch))

(define (bigy)
    '(rock bottle branch))

(define (me)
    '(rock bottle branch))

(define (crate) (drop-between (lambda () (any-of trash)) 1 3))

((eval name))
