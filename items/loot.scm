(define (any-of xs)
    (list-ref xs (random-integer (length xs))))

(define (drop-between f start end)
    (map (lambda (_) (f)) (counter (random-integer-between start (+ end 1)))))

(define (drop-rate x total)
    (lambda ()
        (< (random-integer total) x)))

(define (and-maybe xs rate x)
    (if (rate)
        (cons x xs)
        xs))

(define trash '(rock bottle short_stick stick branch duct_tape))

(define (zob)
    (and-maybe
        (drop-between (lambda () (any-of '(hammer scissors kitchen_knife))) 1 2)
        (drop-rate 1 5)
        'heal_pills))

(define (runner)
    (and-maybe
        (drop-between (lambda () (any-of '(scissors bottle baseball_bat))) 1 2)
        (drop-rate 1 5)
        'heal_pills))

(define (bigy)
    (and-maybe
        (drop-between (lambda () (any-of '(boulder pipe sledgehammer))) 2 3)
        (drop-rate 1 2)
        'heal_pills))

(define (me)
    (and-maybe
        (list (any-of '(meat_cleaver glock lamp)))
        (drop-rate 1 5)
        'heal_pills))

(define (crate) (drop-between (lambda () (any-of trash)) 1 3))

((eval name))
