(define (any-of xs)
    (list-ref xs (random-integer (length xs))))

(define (drop-between f start end)
    (map (lambda (_) (f)) (counter (random-integer-between start (+ end 1)))))

(define (maybe-multiple f rate limit)
    (define (inner current limit)
        (if (> limit 0)
            (if (rate)
                (inner (cons (f) current) (- limit 1))
                current)
            current))
    (inner (list (f)) limit))

(define (drop-rate x total)
    (lambda ()
        (< (random-integer total) x)))

(define (and-maybe xs rate x)
    (if (rate)
        (cons x xs)
        xs))

(define (trash) '(rock bottle short_stick stick branch duct_tape))

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

(define (crate)
    (cond
        ((eq? state 'create) (standard-drops (trash)))
        ((eq? state 'destroy) (standard-drops '(stick short_stick plank)))))

(define (sink)
    (cond
        ((eq? state 'create) '(bottle))
        ((eq? state 'destroy) (standard-drops '(pipe short_stick plank ceramic)))))

(define (cabinet)
    (cond
        ((eq? state 'create) '(heal_pills))
        ((eq? state 'destroy) (standard-drops '(metal_shard)))))

(define (wood_chair) (standard-drops '(stick short_stick plank)))
(define (wood_table) (standard-drops '(stick plank)))
(define (bed) (standard-drops '(stick plank cloth)))

(define (wood) (standard-drops '(stick short_stick plank)))

(define (glass) (standard-drops '(glass_shard)))

(define (concrete) (standard-drops '(rock boulder)))
(define (asphalt) (standard-drops '(rock boulder)))

((eval name))
