(define (any-of xs)
    (list-ref xs (random-integer (length xs))))

(define (drop-between f start end)
    (map (lambda (_) (f)) (counter (random-integer-between start (+ end 1)))))

(define (drop-between-less f start end rate)
    (cond
        ((= start 0) (if (rate) (drop-between-less 1 end rate) '()))
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
        '(lamp)
        (standard-drops '(bottle duct_tape heal_pills))))

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
        ((eq? state 'destroy) (standard-drops '(pipe short_stick plank ceramic_shard)))))

(define (cabinet)
    (cond
        ((eq? state 'create) '(heal_pills))
        ((eq? state 'destroy) (standard-drops '(metal_shard glass_shard)))))

(define (wood_chair) (standard-drops '(stick short_stick plank)))
(define (wood_table) (standard-drops '(stick plank)))
(define (bed) (standard-drops '(stick plank cloth)))

(define (metal_door) (standard-drops '(metal_shard)))

(define (wood) (standard-drops '(stick short_stick plank)))

(define (glass) (standard-drops '(glass_shard)))

(define (concrete) (if ((drop-rate 1 3)) '(boulder) (drop-between-less (lambda () 'rock) 2 4 (drop-rate 1 3))))
(define (asphalt) (if ((drop-rate 1 3)) '(boulder) (drop-between-less (lambda () 'rock) 2 4 (drop-rate 1 3))))

((eval name))
