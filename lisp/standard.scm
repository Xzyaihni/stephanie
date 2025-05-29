(define list (lambda xs xs))

(define (list-tail xs n)
    (if (= n 0)
	xs
	(list-tail (cdr xs) (- n 1))))

(define (list-ref xs n) (car (list-tail xs n)))

(define (counter x)
    (define (counter-inner current)
        (if (< current x)
            (cons current (counter-inner (+ current 1)))
            '()))
    (counter-inner 0))

(define (fold f start xs)
    (if (null? xs)
        start
        (fold f (f (car xs) start) (cdr xs))))

(define (reverse xs) (fold cons '() xs))

(define (map f xs)
    (if (null? xs)
        '()
        (cons (f (car xs)) (map f (cdr xs)))))

(define (zip as bs)
    (if (or (null? as) (null? bs))
        '()
        (cons (cons (car as) (car bs)) (zip (cdr as) (cdr bs)))))

(define (for-each f xs)
    (if (null? xs)
        '()
        (begin
            (f (car xs))
            (for-each f (cdr xs)))))

(define (filter p xs)
    (fold
        (lambda (x acc)
            (if (p x)
                (cons x acc)
                acc))
        '()
        xs))

; does a bubble sort
(define (sort id xs)
    (define (is-sorted xs)
        (if (or (null? xs) (null? (cdr xs)))
	    #t
	    (if (> (id (car xs)) (id (car (cdr xs))))
		#f
		(is-sorted (cdr xs)))))
    (define (swap-one xs)
	(if (null? (cdr xs))
	    xs
	    (let ((a (car xs)) (b (car (cdr xs))))
		(if (> (id a) (id b))
		    (cons b (cons a (cdr (cdr xs))))
		    (cons a (swap-one (cdr xs)))))))
    (define (do-once xs)
	(if (is-sorted xs)
	    xs
	    (do-once (swap-one xs))))
    (do-once xs))


(define (drop xs n)
    (if (or (= n 0) (null? xs))
        xs
        (drop (cdr xs) (- n 1))))

(define (take xs n)
    (if (or (= n 0) (null? xs))
        '()
        (cons (car xs) (take (cdr xs) (- n 1)))))

(define (replicate n x)
    (if (= n 0)
        '()
        (cons x (replicate (- n 1) x))))

(define (repeat f n)
    (if (= n 0)
        '()
        (begin (f) (repeat f (- n 1)))))

(define (length xs)
    (fold (lambda (_ acc) (+ acc 1)) 0 xs))

(define (not x) (if x #f #t))

; technically the specs say it has to be short circuiting but i dont care
(define (or a b)
    (if a
        #t
        (if b
            #t
            #f)))

(define (and a b)
    (if a
        (if b
            #t
            #f)
        #f))

(define (>= a b) (or (> a b) (= a b)))
(define (<= a b) (or (< a b) (= a b)))

(define (abs x)
    (if (< x 0.0)
        (- 0 x)
        x))

(define (square x) (* x x))

(define (expi x p)
    (if (= p 0)
        1
        (if (= p 1)
            x
            (* x (expi x (- p 1))))))

(define (factorial x)
    (if (<= x 0)
        1
        (* x (factorial (- x 1)))))

(define (derivative f)
    (define epsilon 0.0001)
    (lambda (x)
        (let ((low (f (- x epsilon))) (high (f (+ x epsilon))))
            (/ (- high low) (+ epsilon epsilon)))))

(define (newtons-method initial f)
    (define df (derivative f))
    (define (newtons-method-inner x index)
        (let ((current (f x)))
            (if (> index 10000)
                x
                (if (< (abs current) 0.00001)
                    x
                    (newtons-method-inner
                        (- x (/ current (df x)))
                        (+ index 1))))))
    (newtons-method-inner initial 0))

(define exp-iterations 10)

(define (expm1 x)
    (define (expm1-inner i sum)
        (if (> i exp-iterations)
            sum
            (expm1-inner
                (+ i 1.0)
                (+ (/ (expi x i) (factorial i)) sum))))
    (if (= x 0)
        0.0
        (expm1-inner 2.0 (exact->inexact x))))

(define (exp x)
    (+ (expm1 x) 1))

(define (ln x)
    (newtons-method x (lambda (y) (- (exp y) x))))

(define (expt b x)
    (exp (* x (ln b))))

(define (sqrt x)
    (newtons-method 1.0 (lambda (y) (- (* y y) x))))

(define (random-integer-between start end)
    (let ((distance (- end start)))
        (+ start (random-integer distance))))
