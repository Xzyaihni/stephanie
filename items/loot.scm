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

(define (and-always xs x) (cons x xs))

(define (trash) '(rock bottle short_stick stick branch duct_tape))

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

(define (zob)
    (cond
        ((eq? state 'create) (and-maybe
                (standard-drops '(hammer scissors kitchen_knife))
                (drop-rate 1 5)
                'heal_pills))
        ((eq? state 'equip) '())))

(define (smol)
    (cond
        ((eq? state 'create) (and-maybe
                (standard-drops '(branch short_stick rock))
                (drop-rate 1 10)
                'heal_pills))
        ((eq? state 'equip) '())))

(define (old)
    (cond
        ((eq? state 'create) (if ((drop-rate 1 6))
                '(lamp)
                (standard-drops '(bottle duct_tape heal_pills))))
        ((eq? state 'equip) '())))

(define (runner)
    (cond
        ((eq? state 'create) (and-maybe
                (standard-drops '(scissors bottle baseball_bat))
                (drop-rate 1 5)
                'heal_pills))
        ((eq? state 'equip) '(runner_cap))))

(define (bigy)
    (cond
        ((eq? state 'create) (and-maybe
                (standard-drops '(boulder pipe sledgehammer axe))
                (drop-rate 1 2)
                'heal_pills))
        ((eq? state 'equip) '())))

(define (me)
    (cond
        ((eq? state 'create) (and-maybe
                (list (any-of '(meat_cleaver glock lamp)))
                (drop-rate 1 5)
                'heal_pills))
        ((eq? state 'equip) '())))

(define (crate)
    (cond
        ((eq? state 'create) (standard-drops (trash)))
        ((eq? state 'destroy) (points-drops '((short_stick 2) (stick 2) (plank 3)) 4))))

(define (sink)
    (cond
        ((eq? state 'create) '(bottle))
        ((eq? state 'destroy) (points-drops '((short_stick 1) (pipe 2) (ceramic_shard 2) (plank 3)) 4))))

(define (cabinet)
    (cond
        ((eq? state 'create) '(heal_pills))
        ((eq? state 'destroy) (standard-drops '(metal_shard glass_shard)))))

(define (safe)
    (cond
        ((eq? state 'create) '(glock))
        ((eq? state 'destroy) (standard-drops '(metal_shard)))))

(define (wood_chair) (points-drops '((short_stick 1) (stick 2) (plank 3)) 3))
(define (wood_table) (points-drops '((stick 2) (plank 3)) 6))
(define (bed) (points-drops '((cloth 2) (stick 2) (plank 3)) 8))

(define (metal_door) (standard-drops '(metal_shard)))
(define (wood_door) (points-drops '((stick 2) (plank 3)) 6))

(define (wood) (points-drops '((short_stick 2) (stick 2) (plank 3)) 7))

(define (glass) (standard-drops '(glass_shard)))

(define (concrete) (points-drops '((rock 1) (boulder 3)) (random-integer-between 2 5)))
(define (asphalt) (points-drops '((rock 1) (boulder 3)) (random-integer-between 2 5)))

((eval name))
