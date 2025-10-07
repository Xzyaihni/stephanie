(define (any-of xs)
    (list-ref xs (random-integer (length xs))))

(define (drop-between f start end)
    (map (lambda (_) (f)) (counter (random-integer-between start (+ end 1)))))

(define (maybe-multiple f rate limit)
    (f)
    (if (> limit 0)
        (if (rate)
            (maybe-multiple f rate (- limit 1)))))

(define (drop-rate x total)
    (lambda ()
        (< (random-integer total) x)))

(define (and-maybe xs rate x)
    (if (rate)
        (cons x xs)
        xs))

(define trash '(rock bottle short_stick stick branch duct_tape))

(define (standard-drops items)
    (maybe-multiple (lambda () (any-of items)) (drop-rate 1 3) 3))

(define (zob)
    (and-maybe
        (standard-drops '(hammer scissors kitchen_knife))
        (drop-rate 1 5)
        'heal_pills))

(define (smol)
    (and-maybe
        (standard-drops '(branch short_stick rock))
        (drop-rate 1 10)
        'heal_pills))

(define (old)
    (if ((drop-rate 1 6))
        (standard-drops '(bottle duct_tape heal_pills))
        '(lamp)))

(define (runner)
    (and-maybe
        (standard-drops '(scissors bottle baseball_bat))
        (drop-rate 1 5)
        'heal_pills))

(define (bigy)
    (and-maybe
        (standard-drops '(boulder pipe sledgehammer axe))
        (drop-rate 1 2)
        'heal_pills))

(define (me)
    (and-maybe
        (list (any-of '(meat_cleaver glock lamp)))
        (drop-rate 1 5)
        'heal_pills))

(define (crate) (standard-drops trash))

(define (sink) '(bottle))

(define (cabinet) '(heal_pills))

((eval name))
